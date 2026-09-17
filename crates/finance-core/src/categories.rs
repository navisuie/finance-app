//! Spending categories.

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::util::required_text;

pub const KINDS: &[&str] = &["income", "expense", "transfer"];

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub transaction_count: i64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryInput {
    pub name: String,
    pub kind: String,
}

fn validate(input: &CategoryInput) -> Result<(String, String)> {
    let name = required_text("Category name", &input.name, 60)?;
    let kind = input.kind.trim().to_string();
    if !KINDS.contains(&kind.as_str()) {
        return Err(CoreError::Invalid(format!("Unknown category type '{kind}'.")));
    }
    Ok((name, kind))
}

fn map_unique(e: rusqlite::Error, name: &str) -> CoreError {
    if let rusqlite::Error::SqliteFailure(f, _) = &e {
        if f.code == rusqlite::ErrorCode::ConstraintViolation {
            return CoreError::Invalid(format!("A category named '{name}' already exists."));
        }
    }
    CoreError::Db(e)
}

pub fn list(conn: &Connection) -> Result<Vec<Category>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.name, c.kind,
                (SELECT count(*) FROM transactions t WHERE t.category_id = c.id)
         FROM categories c ORDER BY c.kind = 'income' DESC, c.kind, c.name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Category { id: r.get(0)?, name: r.get(1)?, kind: r.get(2)?, transaction_count: r.get(3)? })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn find_by_name(conn: &Connection, name: &str) -> Result<Option<i64>> {
    Ok(conn
        .query_row(
            "SELECT id FROM categories WHERE lower(name) = lower(?1) ORDER BY id LIMIT 1",
            [name.trim()],
            |r| r.get(0),
        )
        .optional()?)
}

pub fn create(conn: &Connection, input: &CategoryInput) -> Result<Category> {
    let (name, kind) = validate(input)?;
    if find_by_name(conn, &name)?.is_some() {
        return Err(CoreError::Invalid(format!("A category named '{name}' already exists.")));
    }
    conn.execute("INSERT INTO categories (name, kind) VALUES (?1, ?2)", params![name, kind])
        .map_err(|e| map_unique(e, &name))?;
    let id = conn.last_insert_rowid();
    Ok(Category { id, name, kind, transaction_count: 0 })
}

pub fn update(conn: &Connection, id: i64, input: &CategoryInput) -> Result<()> {
    let (name, kind) = validate(input)?;
    if let Some(other) = find_by_name(conn, &name)? {
        if other != id {
            return Err(CoreError::Invalid(format!("A category named '{name}' already exists.")));
        }
    }
    let n = conn
        .execute("UPDATE categories SET name = ?1, kind = ?2 WHERE id = ?3", params![name, kind, id])
        .map_err(|e| map_unique(e, &name))?;
    if n == 0 {
        return Err(CoreError::Invalid("Category not found.".into()));
    }
    Ok(())
}

/// Delete a category; its transactions become uncategorized.
pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM categories WHERE id = ?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::tests::conn;

    #[test]
    fn crud_and_duplicates() {
        let c = conn();
        let n0 = list(&c).unwrap().len();
        let cat = create(&c, &CategoryInput { name: "Pets".into(), kind: "expense".into() }).unwrap();
        assert_eq!(list(&c).unwrap().len(), n0 + 1);
        assert!(create(&c, &CategoryInput { name: "pets".into(), kind: "expense".into() }).is_err());
        assert!(create(&c, &CategoryInput { name: "X".into(), kind: "weird".into() }).is_err());
        assert!(update(&c, cat.id, &CategoryInput { name: "Groceries".into(), kind: "expense".into() }).is_err());
        update(&c, cat.id, &CategoryInput { name: "Pet care".into(), kind: "expense".into() }).unwrap();
        assert_eq!(find_by_name(&c, "PET CARE").unwrap(), Some(cat.id));
        delete(&c, cat.id).unwrap();
        assert_eq!(find_by_name(&c, "Pet care").unwrap(), None);
    }
}
