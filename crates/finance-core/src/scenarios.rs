//! Saved calculator scenarios: a name plus whatever inputs that calculator
//! took, stored as JSON. The core doesn't know or care about the shape of
//! any particular calculator's inputs.

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;
use serde_json::Value;

use crate::error::{CoreError, Result};
use crate::util::{now_rfc3339, required_text};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Scenario {
    pub id: i64,
    pub calculator: String,
    pub name: String,
    pub inputs: Value,
    pub created_at: String,
}

fn map_row(r: &Row) -> rusqlite::Result<Scenario> {
    let inputs_json: String = r.get(3)?;
    Ok(Scenario {
        id: r.get(0)?,
        calculator: r.get(1)?,
        name: r.get(2)?,
        inputs: serde_json::from_str(&inputs_json).unwrap_or(Value::Null),
        created_at: r.get(4)?,
    })
}

/// Saved scenarios for one calculator, newest first.
pub fn list(conn: &Connection, calculator: &str) -> Result<Vec<Scenario>> {
    let mut stmt = conn.prepare(
        "SELECT id, calculator, name, inputs_json, created_at FROM scenarios
         WHERE calculator = ?1 ORDER BY created_at DESC, id DESC",
    )?;
    let rows = stmt.query_map([calculator], map_row)?.collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

pub fn create(conn: &Connection, calculator: &str, name: &str, inputs: &Value) -> Result<Scenario> {
    let calculator = required_text("Calculator", calculator, 50)?;
    let name = required_text("Name", name, 100)?;
    let inputs_json = serde_json::to_string(inputs)?;
    let created_at = now_rfc3339();
    conn.execute(
        "INSERT INTO scenarios (calculator, name, inputs_json, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![calculator, name, inputs_json, created_at],
    )?;
    let id = conn.last_insert_rowid();
    conn.query_row(
        "SELECT id, calculator, name, inputs_json, created_at FROM scenarios WHERE id = ?1",
        [id],
        map_row,
    )
    .optional()?
    .ok_or(CoreError::Invalid("Scenario not found.".into()))
}

pub fn delete(conn: &Connection, id: i64) -> Result<()> {
    let n = conn.execute("DELETE FROM scenarios WHERE id = ?1", [id])?;
    if n == 0 {
        return Err(CoreError::Invalid("Scenario not found.".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrate;
    use serde_json::json;

    fn conn() -> Connection {
        let mut c = Connection::open_in_memory().unwrap();
        migrate(&mut c).unwrap();
        c
    }

    #[test]
    fn create_list_delete() {
        let c = conn();
        let a = create(&c, "retirement", "Base case", &json!({"currentAge": 30})).unwrap();
        create(&c, "retirement", "Optimistic", &json!({"currentAge": 30, "returnPct": 9})).unwrap();
        create(&c, "savings_goal", "House", &json!({"targetCents": 5_000_000})).unwrap();

        let retirement = list(&c, "retirement").unwrap();
        assert_eq!(retirement.len(), 2);
        assert_eq!(retirement[0].name, "Optimistic"); // newest first

        let savings = list(&c, "savings_goal").unwrap();
        assert_eq!(savings.len(), 1);

        delete(&c, a.id).unwrap();
        assert_eq!(list(&c, "retirement").unwrap().len(), 1);
        assert!(delete(&c, a.id).is_err());
    }

    #[test]
    fn validation() {
        let c = conn();
        assert!(create(&c, "", "x", &json!({})).is_err());
        assert!(create(&c, "retirement", "  ", &json!({})).is_err());
    }
}
