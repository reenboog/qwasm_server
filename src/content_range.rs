use std::{fmt, str::FromStr};

#[derive(Debug, PartialEq)]
pub struct ContentRange {
	pub start: u64,
	pub end: u64,
	pub length: Option<u64>,
}

#[derive(Debug, PartialEq)]
pub struct Range {
	pub start: u64,
	pub end: u64,
}

impl FromStr for Range {
	type Err = ();

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		if !s.starts_with("bytes=") {
			return Err(());
		}

		let range_part = &s[6..];
		let parts: Vec<&str> = range_part.split('-').collect();

		if parts.len() != 2 {
			return Err(());
		}

		let start = parts[0].parse::<u64>().map_err(|_| ())?;
		let end = parts[1].parse::<u64>().map_err(|_| ())?;

		Ok(Range { start, end })
	}
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
			start: range_start,
			end: range_end,
			length: size,
		})
	}
}

impl fmt::Display for ContentRange {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
		write!(
			f,
			"bytes {}-{}/{}",
			self.start,
			self.end,
			self.length.map_or("*".to_string(), |l| l.to_string())
		)
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
			start: 0,
			end: 499,
			length: Some(1234),
		};
		let result = ContentRange::from_str(input).unwrap();
		assert_eq!(result, expected);
	}

	#[test]
	fn test_valid_content_range_without_size() {
		let input = "bytes 0-499/*";
		let expected = ContentRange {
			start: 0,
			end: 499,
			length: None,
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

	use super::*;

	#[test]
	fn test_valid_range() {
		let range = "bytes=100-200".parse::<Range>().unwrap();
		assert_eq!(
			range,
			Range {
				start: 100,
				end: 200
			}
		);
	}

	#[test]
	fn test_valid_range_single_byte() {
		let range = "bytes=50-50".parse::<Range>().unwrap();
		assert_eq!(range, Range { start: 50, end: 50 });
	}

	#[test]
	fn test_invalid_prefix() {
		let result = "100-200".parse::<Range>();
		assert!(result.is_err());
	}

	#[test]
	fn test_invalid_format_missing_end() {
		let result = "bytes=100-".parse::<Range>();
		assert!(result.is_err());
	}

	#[test]
	fn test_invalid_format_missing_start() {
		let result = "bytes=-200".parse::<Range>();
		assert!(result.is_err());
	}

	#[test]
	fn test_invalid_format_no_dash() {
		let result = "bytes=100200".parse::<Range>();
		assert!(result.is_err());
	}

	#[test]
	fn test_invalid_start_not_a_number() {
		let result = "bytes=abc-200".parse::<Range>();
		assert!(result.is_err());
	}

	#[test]
	fn test_invalid_end_not_a_number() {
		let result = "bytes=100-xyz".parse::<Range>();
		assert!(result.is_err());
	}

	#[test]
	fn test_zero_range() {
		let range = "bytes=0-0".parse::<Range>().unwrap();
		assert_eq!(range, Range { start: 0, end: 0 });
	}

	#[test]
	fn test_large_numbers() {
		let range = "bytes=9223372036854775806-9223372036854775807"
			.parse::<Range>()
			.unwrap();
		assert_eq!(
			range,
			Range {
				start: 9223372036854775806,
				end: 9223372036854775807
			}
		);
	}

	#[test]
	fn test_negative_numbers() {
		let result = "bytes=-100--200".parse::<Range>();
		assert!(result.is_err());
	}
}
