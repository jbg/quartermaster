use crate::error::ApiError;

/// Normalise a scanned barcode into its canonical OpenFoodFacts form.
///
/// Rules:
/// - strip whitespace and any non-digit characters
/// - EAN-13 (13 digits) passes through unchanged
/// - UPC-A (12 digits) is zero-padded to 13 digits
/// - EAN-8 (8 digits) and ITF-14 (14 digits) pass through unchanged
/// - anything else is rejected with `bad_request`
pub fn normalise(input: &str) -> Result<String, ApiError> {
    let digits: String = input.chars().filter(|c| c.is_ascii_digit()).collect();
    match digits.len() {
        8 | 13 | 14 => Ok(digits),
        12 => Ok(format!("0{digits}")),
        other => Err(ApiError::BadRequest(format!(
            "barcode must be 8, 12, 13 or 14 digits (got {other})",
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ean13_passes_through() {
        assert_eq!(normalise("5449000000996").unwrap(), "5449000000996");
    }

    #[test]
    fn upca_is_zero_padded() {
        assert_eq!(normalise("049000050103").unwrap(), "0049000050103");
    }

    #[test]
    fn ean8_passes_through() {
        assert_eq!(normalise("12345670").unwrap(), "12345670");
    }

    #[test]
    fn itf14_passes_through() {
        assert_eq!(normalise("14001234567890").unwrap(), "14001234567890");
    }

    #[test]
    fn strips_whitespace_and_hyphens() {
        assert_eq!(normalise(" 5449-0000-00996 ").unwrap(), "5449000000996");
    }

    #[test]
    fn rejects_letters() {
        // After stripping non-digits: empty string — length 0 is rejected.
        assert!(matches!(normalise("abc"), Err(ApiError::BadRequest(_))));
    }

    #[test]
    fn rejects_wrong_length() {
        assert!(matches!(normalise("12345"), Err(ApiError::BadRequest(_))));
    }

    #[test]
    fn rejects_empty() {
        assert!(matches!(normalise(""), Err(ApiError::BadRequest(_))));
    }
}
