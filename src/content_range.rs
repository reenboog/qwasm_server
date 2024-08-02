use std::str::FromStr;

#[derive(Debug, PartialEq)]
pub struct ContentRange {
	pub range_start: u64,
	pub range_end: u64,
	pub size: Option<u64>,
}

impl FromStr for ContentRange {
	type Err = ();

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let parts: Vec<&str> = s.split_whitespace().collect();
		if parts.len() != 2 || parts[0] != "bytes" {
			return Err(());
		}

		let ranges: Vec<&str> = parts[1].split('/').collect();
		if ranges.len() != 2 {
			return Err(());
		}

		let size = if ranges[1] == "*" {
			None
		} else {
			Some(ranges[1].parse().map_err(|_| ())?)
		};

		let range_parts: Vec<&str> = ranges[0].split('-').collect();
		if range_parts.len() != 2 {
			return Err(());
		}

		let range_start = range_parts[0].parse().map_err(|_| ())?;
		let range_end = range_parts[1].parse().map_err(|_| ())?;

		Ok(ContentRange {
			range_start,
			range_end,
			size,
		})
	}
}

#[cfg(test)]
mod tests {
	use super::ContentRange;
	use std::str::FromStr;

	#[test]
	fn test_valid_content_range_with_size() {
		let input = "bytes 0-499/1234";
		let expected = ContentRange {
			range_start: 0,
			range_end: 499,
			size: Some(1234),
		};
		let result = ContentRange::from_str(input).unwrap();
		assert_eq!(result, expected);
	}

	#[test]
	fn test_valid_content_range_without_size() {
		let input = "bytes 0-499/*";
		let expected = ContentRange {
			range_start: 0,
			range_end: 499,
			size: None,
		};
		let result = ContentRange::from_str(input).unwrap();
		assert_eq!(result, expected);
	}

	#[test]
	fn test_invalid_content_range_no_bytes() {
		let input = "0-499/1234";
		assert!(ContentRange::from_str(input).is_err());
	}

	#[test]
	fn test_invalid_content_range_missing_dash() {
		let input = "bytes 0499/1234";
		assert!(ContentRange::from_str(input).is_err());
	}

	#[test]
	fn test_invalid_content_range_missing_slash() {
		let input = "bytes 0-4991234";
		assert!(ContentRange::from_str(input).is_err());
	}

	#[test]
	fn test_invalid_content_range_non_numeric() {
		let input = "bytes 0-abc/1234";
		assert!(ContentRange::from_str(input).is_err());
	}

	#[test]
	fn test_invalid_content_range_size_non_numeric() {
		let input = "bytes 0-499/abc";
		assert!(ContentRange::from_str(input).is_err());
	}

	#[test]
	fn test_invalid_content_range_incomplete_range() {
		let input = "bytes 0-/1234";
		assert!(ContentRange::from_str(input).is_err());
	}

	#[test]
	fn test_invalid_content_range_empty_string() {
		let input = "";
		assert!(ContentRange::from_str(input).is_err());
	}

	#[test]
	fn test_invalid_content_range_unexpected_whitespace() {
		let input = "bytes 0 - 499 / 1234";
		assert!(ContentRange::from_str(input).is_err());
	}
}
