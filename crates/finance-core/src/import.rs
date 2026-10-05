//! CSV import: detect columns, preview with duplicate detection, commit.
//!
//! The file is parsed in Rust. The UI never sends parsed rows back; on commit
//! the file is re-parsed with the chosen mapping so the database only ever
//! receives validated data.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::money::parse_cents;
use crate::transactions::{check_amount, payee_category_map};
use crate::util::{check_date_range, normalize_payee, now_rfc3339};
use crate::{accounts, categories, settings};

pub const MAX_FILE_BYTES: usize = 10 * 1024 * 1024;
pub const MAX_ROWS: usize = 50_000;
const SAMPLE_ROWS: usize = 25;

/// Supported date formats, in detection priority (US month-first before
/// day-first).
pub const DATE_FORMATS: &[(&str, &str)] = &[
    ("%Y-%m-%d", "2026-09-30"),
    ("%m/%d/%Y", "09/30/2026"),
    ("%m/%d/%y", "09/30/26"),
    ("%d/%m/%Y", "30/09/2026"),
    ("%d/%m/%y", "30/09/26"),
    ("%Y/%m/%d", "2026/09/30"),
    ("%m-%d-%Y", "09-30-2026"),
    ("%d-%m-%Y", "30-09-2026"),
    ("%d.%m.%Y", "30.09.2026"),
    ("%b %d, %Y", "Sep 30, 2026"),
    ("%d %b %Y", "30 Sep 2026"),
    ("%Y%m%d", "20260930"),
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum AmountMapping {
    /// One signed amount column. `negate` flips the sign (for exports where
    /// purchases are positive, as many credit cards do).
    Single { column: usize, negate: bool },
    /// Separate money-out and money-in columns.
    Split { debit_column: usize, credit_column: usize },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvMapping {
    /// Rows to skip before data starts (preamble lines plus the header row).
    pub skip_rows: usize,
    /// Which skipped row holds the column names, if any.
    pub header_row: Option<usize>,
    pub date_column: usize,
    pub date_format: String,
    pub payee_column: usize,
    pub amount: AmountMapping,
    pub category_column: Option<usize>,
    pub notes_column: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detection {
    pub columns: Vec<String>,
    pub sample: Vec<Vec<String>>,
    pub data_row_count: usize,
    /// Best guess; `None` if the file could not be understood.
    pub mapping: Option<CsvMapping>,
    /// True when `mapping` came from the last import into this account.
    pub from_previous_import: bool,
    pub date_formats: Vec<DateFormatOption>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DateFormatOption {
    pub format: &'static str,
    pub example: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowStatus {
    New,
    /// Already imported earlier (same account, date, amount, payee).
    Duplicate,
    /// Looks like an existing transaction entered another way.
    PossibleDuplicate,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRow {
    /// 1-based line number in the file.
    pub line: usize,
    pub posted_on: Option<String>,
    pub amount_cents: Option<i64>,
    pub payee: String,
    pub category_id: Option<i64>,
    pub category_name: Option<String>,
    pub notes: Option<String>,
    pub status: RowStatus,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub rows: Vec<PreviewRow>,
    pub new_count: usize,
    pub duplicate_count: usize,
    pub possible_duplicate_count: usize,
    pub error_count: usize,
    pub first_date: Option<String>,
    pub last_date: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResult {
    pub inserted: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProfile {
    pub id: i64,
    pub name: String,
    pub mapping: CsvMapping,
}

// ---------------------------------------------------------------- parsing

/// Parsed records plus the 1-based file line each one started on. (The CSV
/// reader skips blank lines, so record index and line number can differ.)
struct Table {
    rows: Vec<Vec<String>>,
    lines: Vec<usize>,
}

fn read_rows(text: &str) -> Result<Table> {
    if text.len() > MAX_FILE_BYTES {
        return Err(CoreError::Invalid("That file is larger than 10 MB.".into()));
    }
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(text.as_bytes());
    let mut rows = Vec::new();
    let mut lines = Vec::new();
    // Line numbers from byte offsets (the reader's own line counter does not
    // count skipped blank lines).
    let bytes = text.as_bytes();
    let (mut counted_to, mut line) = (0usize, 1usize);
    for rec in rdr.records() {
        let rec = rec.map_err(|e| CoreError::Invalid(format!("Could not read the CSV file: {e}")))?;
        let mut start = rec.position().map(|p| p.byte() as usize).unwrap_or(counted_to).min(bytes.len());
        // The reported position can point at blank lines preceding the record.
        while start < bytes.len() && (bytes[start] == b'\n' || bytes[start] == b'\r') {
            start += 1;
        }
        if start > counted_to {
            line += bytes[counted_to..start].iter().filter(|b| **b == b'\n').count();
            counted_to = start;
        }
        lines.push(line);
        let row: Vec<String> = rec.iter().map(|s| s.to_string()).collect();
        if row.iter().all(|c| c.is_empty()) {
            // A row of only commas: keep a placeholder, skipped later.
            rows.push(Vec::new());
            continue;
        }
        rows.push(row);
        if rows.len() > MAX_ROWS {
            return Err(CoreError::Invalid(format!("That file has more than {MAX_ROWS} rows.")));
        }
    }
    if rows.iter().all(|r| r.is_empty()) {
        return Err(CoreError::Invalid("That file is empty.".into()));
    }
    Ok(Table { rows, lines })
}

pub fn parse_date_cell(value: &str, format: &str) -> Option<String> {
    let v = value.trim();
    if v.is_empty() {
        return None;
    }
    let try_parse = |s: &str| NaiveDate::parse_from_str(s, format).ok();
    let date = try_parse(v)
        .or_else(|| v.split('T').next().and_then(try_parse))
        .or_else(|| v.split_whitespace().next().and_then(try_parse))?;
    check_date_range(date).ok()?;
    Some(date.format("%Y-%m-%d").to_string())
}

pub fn parse_amount_cell(value: &str) -> Result<Option<i64>> {
    let mut v = value.trim().to_uppercase();
    if v.is_empty() || v == "-" {
        return Ok(None);
    }
    for code in ["USD", "US$"] {
        v = v.replace(code, "");
    }
    let mut v = v.trim().to_string();
    let mut sign = 1;
    if let Some(s) = v.strip_suffix("CR") {
        v = s.trim().to_string();
    } else if let Some(s) = v.strip_suffix("DR") {
        v = s.trim().to_string();
        sign = -1;
    } else if let Some(s) = v.strip_suffix('-') {
        v = s.trim().to_string();
        sign = -1;
    }
    let cents = parse_cents(&v)? * sign;
    check_amount(cents)?;
    Ok(Some(cents))
}

fn cell(row: &[String], idx: usize) -> &str {
    row.get(idx).map(String::as_str).unwrap_or("")
}

fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

// -------------------------------------------------------------- detection

const HEADER_WORDS: &[&str] = &[
    "date", "description", "amount", "payee", "merchant", "debit", "credit", "memo", "category",
    "transaction", "withdrawal", "deposit", "balance", "name", "type",
];

fn looks_like_header(row: &[String]) -> bool {
    let non_empty = row.iter().filter(|c| !c.is_empty()).count();
    if non_empty < 2 {
        return false;
    }
    let lower: Vec<String> = row.iter().map(|c| c.to_lowercase()).collect();
    let has_date = lower.iter().any(|c| c.contains("date"));
    let hits = lower.iter().filter(|c| HEADER_WORDS.iter().any(|w| c.contains(w))).count();
    let numeric = row.iter().filter(|c| parse_amount_cell(c).ok().flatten().is_some()).count();
    has_date && hits >= 2 && numeric == 0
}

fn find_header(rows: &[Vec<String>]) -> Option<usize> {
    rows.iter().take(15).position(|r| looks_like_header(r))
}

fn pick(headers: &[String], prefs: &[&str], exclude: &[&str], taken: &HashSet<usize>) -> Option<usize> {
    let lower: Vec<String> = headers.iter().map(|h| h.to_lowercase()).collect();
    for p in prefs {
        // Exact match first, then substring.
        if let Some(i) = lower.iter().position(|h| h == p) {
            if !taken.contains(&i) {
                return Some(i);
            }
        }
    }
    for p in prefs {
        if let Some(i) = lower.iter().enumerate().position(|(i, h)| {
            h.contains(p) && !exclude.iter().any(|x| h.contains(x)) && !taken.contains(&i)
        }) {
            return Some(i);
        }
    }
    None
}

/// The first format (in priority order) that reads at least 60% of values,
/// so a stray bad row doesn't defeat detection.
fn detect_date_format(values: &[&str]) -> Option<&'static str> {
    let values: Vec<&str> = values.iter().copied().filter(|v| !v.trim().is_empty()).collect();
    if values.is_empty() {
        return None;
    }
    let best = DATE_FORMATS
        .iter()
        .map(|(f, _)| (*f, values.iter().filter(|v| parse_date_cell(v, f).is_some()).count()))
        .max_by_key(|(_, n)| *n)?;
    // Prefer the earliest format among those tied for best.
    let chosen = DATE_FORMATS
        .iter()
        .map(|(f, _)| *f)
        .find(|f| values.iter().filter(|v| parse_date_cell(v, f).is_some()).count() == best.1)?;
    (best.1 * 5 >= values.len() * 3).then_some(chosen)
}

fn column_values(rows: &[Vec<String>], col: usize) -> Vec<&str> {
    rows.iter().filter(|r| !r.is_empty()).map(|r| cell(r, col)).collect()
}

fn fraction<F: Fn(&str) -> bool>(values: &[&str], f: F) -> f64 {
    let non_empty: Vec<&&str> = values.iter().filter(|v| !v.is_empty()).collect();
    if non_empty.is_empty() {
        return 0.0;
    }
    non_empty.iter().filter(|v| f(v)).count() as f64 / non_empty.len() as f64
}

fn guess_mapping(
    headers: Option<&[String]>,
    data: &[Vec<String>],
    ncols: usize,
    account_kind: &str,
    skip_rows: usize,
    header_row: Option<usize>,
) -> Option<CsvMapping> {
    let sample: Vec<Vec<String>> = data.iter().filter(|r| !r.is_empty()).take(200).cloned().collect();
    let mut taken = HashSet::new();

    let (date_col, payee_col, amount, category_col, notes_col);
    if let Some(h) = headers {
        date_col = pick(h, &["date", "posting date", "post date", "posted date", "transaction date", "trans. date", "date"], &["value"], &taken)?;
        taken.insert(date_col);
        payee_col = pick(h, &["payee", "merchant", "description", "name", "transaction", "details"], &["type", "date", "category"], &taken)?;
        taken.insert(payee_col);
        let amount_col = pick(h, &["amount"], &["balance"], &taken);
        amount = match amount_col {
            Some(c) => {
                taken.insert(c);
                AmountMapping::Single { column: c, negate: false }
            }
            None => {
                let d = pick(h, &["debit", "withdrawal", "money out", "paid out", "outflow"], &[], &taken)?;
                taken.insert(d);
                let c = pick(h, &["credit", "deposit", "money in", "paid in", "inflow"], &[], &taken)?;
                taken.insert(c);
                AmountMapping::Split { debit_column: d, credit_column: c }
            }
        };
        category_col = pick(h, &["category"], &[], &taken);
        if let Some(c) = category_col {
            taken.insert(c);
        }
        notes_col = pick(h, &["memo", "notes", "note"], &[], &taken);
    } else {
        // No header: infer from content.
        let score_date = |c: usize| detect_date_format(&column_values(&sample, c)).is_some();
        date_col = (0..ncols).find(|&c| score_date(c))?;
        taken.insert(date_col);
        let is_amount = |v: &str| parse_amount_cell(v).ok().flatten().is_some() && v.contains('.');
        let amount_col = (0..ncols)
            .filter(|c| !taken.contains(c))
            .find(|&c| fraction(&column_values(&sample, c), is_amount) > 0.9)?;
        taken.insert(amount_col);
        amount = AmountMapping::Single { column: amount_col, negate: false };
        payee_col = (0..ncols)
            .filter(|c| !taken.contains(c))
            .filter(|&c| fraction(&column_values(&sample, c), |v| parse_amount_cell(v).ok().flatten().is_none()) > 0.8)
            .max_by_key(|&c| column_values(&sample, c).iter().map(|v| v.len()).sum::<usize>())?;
        category_col = None;
        notes_col = None;
    }

    let date_format = detect_date_format(&column_values(&sample, date_col))?.to_string();

    // Credit card exports often list purchases as positive numbers.
    let amount = match amount {
        AmountMapping::Single { column, .. } => {
            let vals: Vec<i64> = column_values(&sample, column)
                .iter()
                .filter_map(|v| parse_amount_cell(v).ok().flatten())
                .filter(|v| *v != 0)
                .collect();
            let positive = vals.iter().filter(|v| **v > 0).count();
            let negate = account_kind == "credit_card"
                && !vals.is_empty()
                && positive as f64 / vals.len() as f64 > 0.6;
            AmountMapping::Single { column, negate }
        }
        split => split,
    };

    Some(CsvMapping {
        skip_rows,
        header_row,
        date_column: date_col,
        date_format,
        payee_column: payee_col,
        amount,
        category_column: category_col,
        notes_column: notes_col,
    })
}

fn last_mapping_key(account_id: i64) -> String {
    format!("import_mapping:account:{account_id}")
}

pub fn detect(conn: &Connection, account_id: i64, text: &str) -> Result<Detection> {
    let account = accounts::get(conn, account_id)?;
    let Table { rows, .. } = read_rows(text)?;
    let header_row = find_header(&rows);
    let skip_rows = header_row.map(|h| h + 1).unwrap_or(0);
    let data = &rows[skip_rows.min(rows.len())..];
    let ncols = rows.iter().map(Vec::len).max().unwrap_or(0);

    let columns: Vec<String> = match header_row {
        Some(h) => (0..ncols)
            .map(|i| {
                let name = cell(&rows[h], i);
                if name.is_empty() { format!("Column {}", i + 1) } else { name.to_string() }
            })
            .collect(),
        None => (0..ncols).map(|i| format!("Column {}", i + 1)).collect(),
    };

    let previous: Option<CsvMapping> = settings::get(conn, &last_mapping_key(account_id))?
        .and_then(|j| serde_json::from_str(&j).ok())
        .filter(|m: &CsvMapping| mapping_fits(m, ncols) && preview_rows_ok(&rows, m));

    let (mapping, from_previous_import) = match previous {
        Some(m) => (Some(m), true),
        None => (
            guess_mapping(
                header_row.map(|h| &rows[h][..]),
                data,
                ncols,
                &account.kind,
                skip_rows,
                header_row,
            ),
            false,
        ),
    };

    Ok(Detection {
        columns,
        sample: data.iter().filter(|r| !r.is_empty()).take(SAMPLE_ROWS).cloned().collect(),
        data_row_count: data.iter().filter(|r| !r.is_empty()).count(),
        mapping,
        from_previous_import,
        date_formats: DATE_FORMATS.iter().map(|(f, e)| DateFormatOption { format: f, example: e }).collect(),
    })
}

fn mapping_fits(m: &CsvMapping, ncols: usize) -> bool {
    let mut cols = vec![m.date_column, m.payee_column];
    match m.amount {
        AmountMapping::Single { column, .. } => cols.push(column),
        AmountMapping::Split { debit_column, credit_column } => {
            cols.push(debit_column);
            cols.push(credit_column);
        }
    }
    cols.extend(m.category_column);
    cols.extend(m.notes_column);
    cols.iter().all(|c| *c < ncols)
}

/// A saved mapping is reused only if it parses at least 60% of sample rows.
fn preview_rows_ok(rows: &[Vec<String>], m: &CsvMapping) -> bool {
    let data: Vec<&Vec<String>> = rows.iter().skip(m.skip_rows).filter(|r| !r.is_empty()).take(50).collect();
    if data.is_empty() {
        return false;
    }
    let ok = data.iter().filter(|r| parse_row(r, m).is_ok()).count();
    ok * 5 >= data.len() * 3
}

// ---------------------------------------------------------------- preview

struct Parsed {
    posted_on: String,
    amount_cents: i64,
    payee: String,
    category: Option<String>,
    notes: Option<String>,
}

fn parse_row(row: &[String], m: &CsvMapping) -> std::result::Result<Parsed, String> {
    let raw_date = cell(row, m.date_column);
    let posted_on = parse_date_cell(raw_date, &m.date_format)
        .ok_or_else(|| format!("Can't read date '{raw_date}'"))?;
    let amount_cents = match m.amount {
        AmountMapping::Single { column, negate } => {
            let v = parse_amount_cell(cell(row, column)).map_err(|e| e.to_string())?;
            let v = v.ok_or_else(|| "Amount is empty".to_string())?;
            if negate { -v } else { v }
        }
        AmountMapping::Split { debit_column, credit_column } => {
            let d = parse_amount_cell(cell(row, debit_column)).map_err(|e| e.to_string())?;
            let c = parse_amount_cell(cell(row, credit_column)).map_err(|e| e.to_string())?;
            match (d, c) {
                (Some(d), None) | (Some(d), Some(0)) if d != 0 => -d.abs(),
                (None, Some(c)) | (Some(0), Some(c)) => c.abs(),
                (Some(0), None) | (None, None) => return Err("No amount in debit or credit".into()),
                (Some(_), Some(_)) => return Err("Both debit and credit have amounts".into()),
                _ => return Err("No amount".into()),
            }
        }
    };
    let payee = collapse_ws(cell(row, m.payee_column));
    if payee.is_empty() {
        return Err("Description is empty".into());
    }
    let payee: String = payee.chars().take(200).collect();
    let category = m.category_column.map(|c| cell(row, c).to_string()).filter(|s| !s.is_empty());
    let notes = m
        .notes_column
        .map(|c| collapse_ws(cell(row, c)))
        .filter(|s| !s.is_empty())
        .map(|s| s.chars().take(1000).collect());
    Ok(Parsed { posted_on, amount_cents, payee, category, notes })
}

fn import_key(date: &str, cents: i64, payee: &str, occurrence: usize) -> String {
    format!("{date}|{cents}|{}|{occurrence}", payee.to_lowercase())
}

struct Prepared {
    rows: Vec<PreviewRow>,
    keys: HashMap<usize, String>,
}

fn prepare(conn: &Connection, account_id: i64, text: &str, m: &CsvMapping) -> Result<Prepared> {
    accounts::get(conn, account_id)?;
    let Table { rows, lines } = read_rows(text)?;
    let ncols = rows.iter().map(Vec::len).max().unwrap_or(0);
    if !mapping_fits(m, ncols) {
        return Err(CoreError::Invalid("The column mapping doesn't match this file.".into()));
    }
    if !DATE_FORMATS.iter().any(|(f, _)| *f == m.date_format) {
        return Err(CoreError::Invalid("Unsupported date format.".into()));
    }

    let cat_names: HashMap<String, (i64, String)> = categories::list(conn)?
        .into_iter()
        .map(|c| (c.name.to_lowercase(), (c.id, c.name)))
        .collect();
    let cat_by_id: HashMap<i64, String> = cat_names.values().cloned().collect();
    let learned = payee_category_map(conn)?;

    let mut exact = conn.prepare_cached(
        "SELECT EXISTS(SELECT 1 FROM transactions WHERE account_id = ?1 AND import_hash = ?2)",
    )?;
    let mut similar = conn.prepare_cached(
        "SELECT payee, source FROM transactions
         WHERE account_id = ?1 AND posted_on = ?2 AND amount_cents = ?3",
    )?;

    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut out = Vec::new();
    let mut keys = HashMap::new();
    for (i, row) in rows.iter().enumerate().skip(m.skip_rows) {
        if row.is_empty() {
            continue;
        }
        let line = lines[i];
        match parse_row(row, m) {
            Err(msg) => out.push(PreviewRow {
                line,
                posted_on: None,
                amount_cents: None,
                payee: collapse_ws(cell(row, m.payee_column)),
                category_id: None,
                category_name: None,
                notes: None,
                status: RowStatus::Error,
                message: Some(msg),
            }),
            Ok(p) => {
                let base = import_key(&p.posted_on, p.amount_cents, &p.payee, 0);
                let n = seen.entry(base).or_insert(0);
                let key = import_key(&p.posted_on, p.amount_cents, &p.payee, *n);
                *n += 1;

                let is_dup: bool = exact.query_row(params![account_id, key], |r| r.get(0))?;
                let status = if is_dup {
                    RowStatus::Duplicate
                } else {
                    let norm = normalize_payee(&p.payee);
                    let mut rows = similar.query(params![account_id, p.posted_on, p.amount_cents])?;
                    let mut possible = false;
                    while let Some(r) = rows.next()? {
                        let (payee, source): (String, String) = (r.get(0)?, r.get(1)?);
                        if source != "csv" || normalize_payee(&payee) == norm {
                            possible = true;
                            break;
                        }
                    }
                    if possible { RowStatus::PossibleDuplicate } else { RowStatus::New }
                };

                let category_id = p
                    .category
                    .as_ref()
                    .and_then(|c| cat_names.get(&c.to_lowercase()).map(|(id, _)| *id))
                    .or_else(|| learned.get(&normalize_payee(&p.payee)).copied());
                let message = match status {
                    RowStatus::Duplicate => Some("Already imported".to_string()),
                    RowStatus::PossibleDuplicate => {
                        Some("A transaction with the same date and amount exists".to_string())
                    }
                    _ => None,
                };
                keys.insert(line, key);
                out.push(PreviewRow {
                    line,
                    posted_on: Some(p.posted_on),
                    amount_cents: Some(p.amount_cents),
                    payee: p.payee,
                    category_name: category_id.and_then(|id| cat_by_id.get(&id).cloned()),
                    category_id,
                    notes: p.notes,
                    status,
                    message,
                });
            }
        }
    }
    if out.is_empty() {
        return Err(CoreError::Invalid("No transactions found in that file.".into()));
    }
    Ok(Prepared { rows: out, keys })
}

pub fn preview(conn: &Connection, account_id: i64, text: &str, m: &CsvMapping) -> Result<Preview> {
    let p = prepare(conn, account_id, text, m)?;
    let count = |s: RowStatus| p.rows.iter().filter(|r| r.status == s).count();
    let dates: Vec<&String> = p.rows.iter().filter_map(|r| r.posted_on.as_ref()).collect();
    Ok(Preview {
        new_count: count(RowStatus::New),
        duplicate_count: count(RowStatus::Duplicate),
        possible_duplicate_count: count(RowStatus::PossibleDuplicate),
        error_count: count(RowStatus::Error),
        first_date: dates.iter().min().map(|s| s.to_string()),
        last_date: dates.iter().max().map(|s| s.to_string()),
        rows: p.rows,
    })
}

/// Import the selected lines. Exact duplicates and error rows are always
/// skipped. `category_overrides` maps line number to category (or none).
pub fn commit(
    conn: &mut Connection,
    account_id: i64,
    text: &str,
    m: &CsvMapping,
    lines: &[usize],
    category_overrides: &HashMap<usize, Option<i64>>,
) -> Result<ImportResult> {
    let prepared = prepare(conn, account_id, text, m)?;
    let wanted: HashSet<usize> = lines.iter().copied().collect();
    let valid_cats: HashSet<i64> = categories::list(conn)?.into_iter().map(|c| c.id).collect();
    let now = now_rfc3339();

    // Recurring payments you've already tagged (a car loan, a card payment)
    // get tagged again, so they stay out of spending without re-doing it.
    let learned_transfers = crate::transactions::payee_transfer_map(conn)?;

    let tx = conn.transaction()?;
    let mut inserted = 0;
    let mut skipped = 0;
    {
        let mut stmt = tx.prepare(
            "INSERT OR IGNORE INTO transactions
               (account_id, posted_on, amount_cents, payee, category_id, notes, source, import_hash, created_at,
                transfer_account_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'csv', ?7, ?8, ?9)",
        )?;
        for r in &prepared.rows {
            let importable = matches!(r.status, RowStatus::New | RowStatus::PossibleDuplicate);
            if !wanted.contains(&r.line) || !importable {
                skipped += 1;
                continue;
            }
            let category = match category_overrides.get(&r.line) {
                Some(Some(id)) if valid_cats.contains(id) => Some(*id),
                Some(Some(_)) => return Err(CoreError::Invalid("Category not found.".into())),
                Some(None) => None,
                None => r.category_id,
            };
            // Only money going out can be a payment to another account, and
            // never to the account it came from.
            let transfer_to = r
                .amount_cents
                .filter(|c| *c < 0)
                .and_then(|_| learned_transfers.get(&(account_id, normalize_payee(&r.payee))).copied())
                .filter(|target| *target != account_id);
            let n = stmt.execute(params![
                account_id,
                r.posted_on,
                r.amount_cents,
                r.payee,
                category,
                r.notes,
                prepared.keys[&r.line],
                now,
                transfer_to
            ])?;
            if n == 1 { inserted += 1 } else { skipped += 1 }
        }
    }
    tx.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![last_mapping_key(account_id), serde_json::to_string(m)?],
    )?;
    tx.commit()?;
    Ok(ImportResult { inserted, skipped })
}

// --------------------------------------------------------------- profiles

pub fn list_profiles(conn: &Connection) -> Result<Vec<ImportProfile>> {
    let mut stmt = conn.prepare(
        "SELECT id, institution, column_mapping_json FROM import_profiles ORDER BY institution COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
    let mut out = Vec::new();
    for row in rows {
        let (id, name, json) = row?;
        if let Ok(mapping) = serde_json::from_str(&json) {
            out.push(ImportProfile { id, name, mapping });
        }
    }
    Ok(out)
}

pub fn save_profile(conn: &Connection, name: &str, m: &CsvMapping) -> Result<ImportProfile> {
    let name = crate::util::required_text("Profile name", name, 60)?;
    conn.execute(
        "INSERT INTO import_profiles (institution, column_mapping_json) VALUES (?1, ?2)
         ON CONFLICT(institution) DO UPDATE SET column_mapping_json = excluded.column_mapping_json",
        params![name, serde_json::to_string(m)?],
    )?;
    let id: i64 = conn
        .query_row("SELECT id FROM import_profiles WHERE institution = ?1", [&name], |r| r.get(0))
        .optional()?
        .ok_or(CoreError::Crypto)?;
    Ok(ImportProfile { id, name, mapping: m.clone() })
}

pub fn delete_profile(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM import_profiles WHERE id = ?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::tests::{conn, input};
    use crate::transactions::{self, tests::tx, TransactionFilter};

    const BANK: &str = "\u{feff}Details,Posting Date,Description,Amount,Type,Balance\n\
DEBIT,09/02/2026,\"STARBUCKS #1234  SEATTLE\",-5.75,DEBIT_CARD,1000.00\n\
CREDIT,09/01/2026,PAYROLL ACME CORP,\"2,500.00\",ACH_CREDIT,1005.75\n\
DEBIT,09/02/2026,\"STARBUCKS #1234  SEATTLE\",-5.75,DEBIT_CARD,994.25\n\
\n\
DEBIT,not a date,BROKEN ROW,-1.00,DEBIT_CARD,0\n";

    const CARD_POSITIVE: &str = "Date,Description,Card Member,Amount\n\
09/03/2026,WHOLE FOODS,NAVI,82.10\n\
09/04/2026,NETFLIX,NAVI,15.49\n\
09/05/2026,PAYMENT THANK YOU,NAVI,-500.00\n";

    const SPLIT_WITH_PREAMBLE: &str = "Account: Everyday Checking\nExported 2026-09-10\n\n\
Date,Description,Withdrawals,Deposits,Category\n\
2026-09-06,Rent,1800.00,,Housing\n\
2026-09-07,Refund,,25.00,Shopping\n\
2026-09-08,Both,1.00,2.00,\n";

    const NO_HEADER: &str = "20260901,COSTCO WHOLESALE,-120.44\n20260902,SHELL OIL,-40.00\n";

    fn setup(kind: &str) -> (Connection, i64) {
        let mut c = conn();
        let a = accounts::create(&mut c, &input("Test", kind)).unwrap();
        (c, a.id)
    }

    fn all_lines(p: &Preview) -> Vec<usize> {
        p.rows.iter().map(|r| r.line).collect()
    }

    const LOAN_PAYMENTS: &str = "Date,Description,Amount\n\
09/05/2026,HONDA FINANCIAL SVC #4821,-700.00\n\
09/06/2026,PAYROLL ACME CORP,2500.00\n";

    #[test]
    fn an_imported_recurring_payment_keeps_the_account_it_pays() {
        let (mut c, acct) = setup("checking");
        let loan = accounts::create(&mut c, &crate::accounts::tests::input("Car loan", "loan")).unwrap();

        // Tag last month's payment by hand, the way the form does.
        let mut tagged = crate::transactions::tests::tx(acct, "2026-08-05", -70_000, "HONDA FINANCIAL SVC #1109");
        tagged.transfer_account_id = Some(loan.id);
        crate::transactions::create(&c, &tagged).unwrap();

        let m = detect(&c, acct, LOAN_PAYMENTS).unwrap().mapping.unwrap();
        let p = preview(&c, acct, LOAN_PAYMENTS, &m).unwrap();
        commit(&mut c, acct, LOAN_PAYMENTS, &m, &all_lines(&p), &HashMap::new()).unwrap();

        let page = crate::transactions::query(&c, &Default::default()).unwrap();
        let imported = page.items.iter().find(|t| t.posted_on == "2026-09-05").unwrap();
        assert_eq!(imported.transfer_account_id, Some(loan.id), "the recurring payment is tagged for you");
        assert_eq!(imported.transfer_account_name.as_deref(), Some("Car loan"));

        // Income from an unrelated payee stays untagged.
        let payroll = page.items.iter().find(|t| t.payee.contains("PAYROLL")).unwrap();
        assert_eq!(payroll.transfer_account_id, None);

        // And it's out of spending, like any tagged transfer.
        let s =
            crate::transactions::summary(&c, "2026-09-01", "2026-09-30", crate::accounts::Scope::Real).unwrap();
        assert_eq!(s.spending_cents, 0);
    }

    #[test]
    fn amount_cells() {
        assert_eq!(parse_amount_cell("12.34 CR").unwrap(), Some(1234));
        assert_eq!(parse_amount_cell("12.34 DR").unwrap(), Some(-1234));
        assert_eq!(parse_amount_cell("12.34-").unwrap(), Some(-1234));
        assert_eq!(parse_amount_cell("USD 1,000.00").unwrap(), Some(100000));
        assert_eq!(parse_amount_cell("").unwrap(), None);
        assert!(parse_amount_cell("abc").is_err());
    }

    #[test]
    fn date_cells() {
        assert_eq!(parse_date_cell("09/02/2026", "%m/%d/%Y").as_deref(), Some("2026-09-02"));
        assert_eq!(parse_date_cell("2026-09-02T10:00:00Z", "%Y-%m-%d").as_deref(), Some("2026-09-02"));
        assert_eq!(parse_date_cell("09/02/2026 11:30 AM", "%m/%d/%Y").as_deref(), Some("2026-09-02"));
        assert_eq!(parse_date_cell("Sep 2, 2026", "%b %d, %Y").as_deref(), Some("2026-09-02"));
        assert_eq!(parse_date_cell("13/31/2026", "%m/%d/%Y"), None);
    }

    #[test]
    fn detects_bank_export_and_imports_with_dedupe() {
        let (mut c, acct) = setup("checking");
        let d = detect(&c, acct, BANK).unwrap();
        let m = d.mapping.clone().unwrap();
        assert_eq!(d.columns[2], "Description");
        assert_eq!(m.date_column, 1);
        assert_eq!(m.payee_column, 2);
        assert_eq!(m.amount, AmountMapping::Single { column: 3, negate: false });
        assert_eq!(m.date_format, "%m/%d/%Y");
        assert_eq!((m.skip_rows, m.header_row), (1, Some(0)));
        assert!(!d.from_previous_import);
        assert_eq!(d.data_row_count, 4);

        let p = preview(&c, acct, BANK, &m).unwrap();
        assert_eq!((p.new_count, p.error_count, p.duplicate_count), (3, 1, 0));
        assert_eq!(p.first_date.as_deref(), Some("2026-09-01"));
        let err = p.rows.iter().find(|r| r.status == RowStatus::Error).unwrap();
        assert_eq!(err.line, 6);
        assert_eq!(p.rows[0].payee, "STARBUCKS #1234 SEATTLE");
        assert_eq!(p.rows[1].amount_cents, Some(250000));

        let r = commit(&mut c, acct, BANK, &m, &all_lines(&p), &HashMap::new()).unwrap();
        // Two identical Starbucks rows are both kept (same-day repeat purchase).
        assert_eq!((r.inserted, r.skipped), (3, 1));

        // Re-importing the same file finds everything already there.
        let p2 = preview(&c, acct, BANK, &m).unwrap();
        assert_eq!((p2.new_count, p2.duplicate_count), (0, 3));
        let r2 = commit(&mut c, acct, BANK, &m, &all_lines(&p2), &HashMap::new()).unwrap();
        assert_eq!(r2.inserted, 0);

        // The mapping is remembered for this account.
        let d2 = detect(&c, acct, BANK).unwrap();
        assert!(d2.from_previous_import);
        assert_eq!(d2.mapping.unwrap(), m);
    }

    #[test]
    fn credit_card_positive_purchases_are_negated() {
        let (c, acct) = setup("credit_card");
        let m = detect(&c, acct, CARD_POSITIVE).unwrap().mapping.unwrap();
        assert_eq!(m.amount, AmountMapping::Single { column: 3, negate: true });
        assert_eq!(m.payee_column, 1);
        let p = preview(&c, acct, CARD_POSITIVE, &m).unwrap();
        assert_eq!(p.rows[0].amount_cents, Some(-8210));
        assert_eq!(p.rows[2].amount_cents, Some(50000));
    }

    #[test]
    fn split_columns_preamble_and_categories() {
        let (mut c, acct) = setup("checking");
        let d = detect(&c, acct, SPLIT_WITH_PREAMBLE).unwrap();
        let m = d.mapping.unwrap();
        assert_eq!(m.header_row, Some(2)); // record index; the blank line is skipped
        assert_eq!(m.skip_rows, 3);
        assert_eq!(m.amount, AmountMapping::Split { debit_column: 2, credit_column: 3 });
        assert_eq!(m.category_column, Some(4));
        assert_eq!(m.date_format, "%Y-%m-%d");
        let p = preview(&c, acct, SPLIT_WITH_PREAMBLE, &m).unwrap();
        assert_eq!(p.rows[0].amount_cents, Some(-180000));
        assert_eq!(p.rows[0].category_name.as_deref(), Some("Housing"));
        assert_eq!(p.rows[1].amount_cents, Some(2500));
        assert_eq!(p.rows[2].status, RowStatus::Error);

        // Override line 5's category to none and only import that line.
        let mut ov = HashMap::new();
        ov.insert(5, None);
        let r = commit(&mut c, acct, SPLIT_WITH_PREAMBLE, &m, &[5], &ov).unwrap();
        assert_eq!(r.inserted, 1);
        let page = transactions::query(&c, &TransactionFilter::default()).unwrap();
        assert_eq!(page.items[0].category_id, None);
        // Unknown category override is rejected.
        let mut bad = HashMap::new();
        bad.insert(6, Some(99_999));
        assert!(commit(&mut c, acct, SPLIT_WITH_PREAMBLE, &m, &[6], &bad).is_err());
    }

    #[test]
    fn headerless_file() {
        let (c, acct) = setup("checking");
        let d = detect(&c, acct, NO_HEADER).unwrap();
        assert_eq!(d.columns, vec!["Column 1", "Column 2", "Column 3"]);
        let m = d.mapping.unwrap();
        assert_eq!((m.date_column, m.payee_column, m.skip_rows), (0, 1, 0));
        assert_eq!(m.date_format, "%Y%m%d");
        assert_eq!(preview(&c, acct, NO_HEADER, &m).unwrap().new_count, 2);
    }

    #[test]
    fn learned_categories_and_possible_duplicates() {
        let (mut c, acct) = setup("checking");
        let dining = categories::find_by_name(&c, "Dining").unwrap().unwrap();
        let mut manual = tx(acct, "2026-09-02", -575, "Coffee");
        manual.category_id = Some(dining);
        transactions::create(&c, &manual).unwrap();
        let mut earlier = tx(acct, "2026-08-01", -400, "STARBUCKS #999 SEATTLE");
        earlier.category_id = Some(dining);
        transactions::create(&c, &earlier).unwrap();

        let m = detect(&c, acct, BANK).unwrap().mapping.unwrap();
        let p = preview(&c, acct, BANK, &m).unwrap();
        assert_eq!(p.rows[0].category_id, Some(dining), "learned from payee history");
        assert_eq!(p.rows[0].status, RowStatus::PossibleDuplicate);
        assert_eq!(p.possible_duplicate_count, 2);

        // Possible duplicates import only when selected.
        let r = commit(&mut c, acct, BANK, &m, &[3], &HashMap::new()).unwrap();
        assert_eq!(r.inserted, 1);
    }

    #[test]
    fn rejects_bad_input() {
        let (c, acct) = setup("checking");
        assert!(detect(&c, acct, "").is_err());
        assert!(detect(&c, 999, BANK).is_err());
        let big = "a".repeat(MAX_FILE_BYTES + 1);
        assert!(detect(&c, acct, &big).is_err());
        let m = detect(&c, acct, BANK).unwrap().mapping.unwrap();
        let mut wrong = m.clone();
        wrong.payee_column = 42;
        assert!(preview(&c, acct, BANK, &wrong).is_err());
        let mut fmt = m;
        fmt.date_format = "%s".into();
        assert!(preview(&c, acct, BANK, &fmt).is_err());
        // Garbage file yields no mapping rather than a crash.
        let d = detect(&c, acct, "hello\nworld\n").unwrap();
        assert!(d.mapping.is_none());
    }

    #[test]
    fn profiles() {
        let (c, acct) = setup("checking");
        let m = detect(&c, acct, BANK).unwrap().mapping.unwrap();
        let p = save_profile(&c, "My bank", &m).unwrap();
        save_profile(&c, "My bank", &m).unwrap();
        assert_eq!(list_profiles(&c).unwrap().len(), 1);
        assert_eq!(list_profiles(&c).unwrap()[0].mapping, m);
        delete_profile(&c, p.id).unwrap();
        assert!(list_profiles(&c).unwrap().is_empty());
        assert!(save_profile(&c, " ", &m).is_err());
    }

    #[test]
    fn mapping_json_shape() {
        let m = CsvMapping {
            skip_rows: 1,
            header_row: Some(0),
            date_column: 0,
            date_format: "%Y-%m-%d".into(),
            payee_column: 1,
            amount: AmountMapping::Split { debit_column: 2, credit_column: 3 },
            category_column: None,
            notes_column: None,
        };
        let j = serde_json::to_value(&m).unwrap();
        assert_eq!(j["amount"]["mode"], "split");
        assert_eq!(j["amount"]["debitColumn"], 2);
        assert_eq!(j["skipRows"], 1);
    }
}
