//! Map Plaid account types and categories onto this app's.

/// Suggested account kind for a Plaid account type/subtype.
pub fn account_kind(account_type: &str, subtype: Option<&str>) -> &'static str {
    let sub = subtype.unwrap_or("").to_ascii_lowercase();
    match account_type {
        "depository" => match sub.as_str() {
            "checking" | "prepaid" | "paypal" | "ebt" => "checking",
            "hsa" => "hsa",
            "savings" | "money market" | "cd" | "cash management" => "savings",
            _ => "other",
        },
        "credit" => "credit_card",
        "loan" => match sub.as_str() {
            "mortgage" | "home equity" => "mortgage",
            _ => "loan",
        },
        "investment" | "brokerage" => match sub.as_str() {
            "401k" | "401a" | "403b" | "457b" | "roth 401k" | "thrift savings plan" | "keogh"
            | "profit sharing plan" | "retirement" | "stock plan" => "401k",
            "roth" => "roth_ira",
            "ira" | "sep ira" | "simple ira" | "sarsep" | "rollover" | "lira" | "rrsp" => {
                "traditional_ira"
            }
            "hsa" => "hsa",
            _ => "brokerage",
        },
        _ => "other",
    }
}

/// Default category name for a Plaid personal finance category (v1 or v2).
/// Returns `None` when there is no sensible match.
pub fn category_name(primary: &str, detailed: Option<&str>) -> Option<&'static str> {
    let d = detailed.unwrap_or("");
    Some(match primary {
        "INCOME" => "Income",
        "TRANSFER_IN" | "TRANSFER_OUT" | "LOAN_DISBURSEMENTS" => "Transfer",
        "LOAN_PAYMENTS" => match d {
            "LOAN_PAYMENTS_MORTGAGE_PAYMENT" => "Housing",
            "LOAN_PAYMENTS_CAR_PAYMENT" => "Transportation",
            "LOAN_PAYMENTS_STUDENT_LOAN_PAYMENT" => "Education",
            _ => "Transfer",
        },
        "BANK_FEES" => "Fees",
        "ENTERTAINMENT" => "Entertainment",
        "FOOD_AND_DRINK" => {
            if d == "FOOD_AND_DRINK_GROCERIES" {
                "Groceries"
            } else {
                "Dining"
            }
        }
        "GENERAL_MERCHANDISE" => "Shopping",
        "HOME_IMPROVEMENT" => "Housing",
        "MEDICAL" | "PERSONAL_CARE" => "Health",
        "GENERAL_SERVICES" => match d {
            "GENERAL_SERVICES_INSURANCE" => "Insurance",
            "GENERAL_SERVICES_EDUCATION" => "Education",
            "GENERAL_SERVICES_AUTOMOTIVE" => "Transportation",
            _ => "Other",
        },
        "GOVERNMENT_AND_NON_PROFIT" => match d {
            "GOVERNMENT_AND_NON_PROFIT_TAX_PAYMENT" => "Taxes",
            "GOVERNMENT_AND_NON_PROFIT_DONATIONS" => "Gifts & Donations",
            _ => "Other",
        },
        "TRANSPORTATION" => "Transportation",
        "TRAVEL" => "Travel",
        "RENT_AND_UTILITIES" => {
            if d == "RENT_AND_UTILITIES_RENT" {
                "Housing"
            } else {
                "Utilities"
            }
        }
        _ => return None,
    })
}

/// Plaid amounts are dollars as floats with positive = money out. Convert to
/// this app's signed cents (negative = money out).
pub fn to_cents(plaid_amount: f64) -> Option<i64> {
    if !plaid_amount.is_finite() || plaid_amount.abs() > 1e10 {
        return None;
    }
    Some(-((plaid_amount * 100.0).round() as i64))
}

/// Balance in cents as shown by the institution (not sign-flipped).
pub fn balance_cents(amount: f64) -> Option<i64> {
    if !amount.is_finite() || amount.abs() > 1e10 {
        return None;
    }
    Some((amount * 100.0).round() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds() {
        assert_eq!(account_kind("depository", Some("checking")), "checking");
        assert_eq!(account_kind("depository", Some("savings")), "savings");
        assert_eq!(account_kind("credit", Some("credit card")), "credit_card");
        assert_eq!(account_kind("loan", Some("mortgage")), "mortgage");
        assert_eq!(account_kind("loan", Some("student")), "loan");
        assert_eq!(account_kind("investment", Some("401k")), "401k");
        assert_eq!(account_kind("investment", Some("roth")), "roth_ira");
        assert_eq!(account_kind("investment", Some("ira")), "traditional_ira");
        assert_eq!(account_kind("investment", Some("brokerage")), "brokerage");
        assert_eq!(account_kind("investment", None), "brokerage");
        assert_eq!(account_kind("other", Some("x")), "other");
    }

    #[test]
    fn categories() {
        assert_eq!(category_name("FOOD_AND_DRINK", Some("FOOD_AND_DRINK_GROCERIES")), Some("Groceries"));
        assert_eq!(category_name("FOOD_AND_DRINK", Some("FOOD_AND_DRINK_COFFEE")), Some("Dining"));
        assert_eq!(category_name("LOAN_PAYMENTS", Some("LOAN_PAYMENTS_CREDIT_CARD_PAYMENT")), Some("Transfer"));
        assert_eq!(category_name("RENT_AND_UTILITIES", Some("RENT_AND_UTILITIES_RENT")), Some("Housing"));
        assert_eq!(category_name("INCOME", Some("INCOME_SALARY")), Some("Income"));
        assert_eq!(category_name("OTHER", None), None);
        assert_eq!(category_name("SOMETHING_NEW", None), None);
    }

    #[test]
    fn cents() {
        assert_eq!(to_cents(12.34), Some(-1234));
        assert_eq!(to_cents(-2500.0), Some(250000));
        assert_eq!(to_cents(0.1 + 0.2), Some(-30));
        assert_eq!(to_cents(f64::NAN), None);
        assert_eq!(balance_cents(1234.567), Some(123457));
    }
}
