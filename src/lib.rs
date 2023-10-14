use std::{error::Error, fmt::Display};
use geo::{Coord, LineString};
use serde::{Serialize, Deserialize};

/// How many digits the coordinates should be rounded to. `Two` works well for metric CRS like Pseudo-mercator EPSG:3857. `Seven` is needed for lat/lon coordinates (WGS-84 aka EPSG:4326). `Other` variant sets arbitrary precision.
pub enum Precision {
    Two, Seven, Other(u8),
}

impl Precision {
	pub fn multiplicator(&self) -> f64 {
		match self {
			Self::Two => 1e2,
			Self::Seven => 1e7,
			Self::Other(v) => 10_f64.powi(*v as i32),
		}
	}
}

// encodes data, writing it to the output vector (which should be &mut, to keep just 1 vec and minimize allocations of vecs)
fn encode_int(value: i64, output: &mut Vec<u8>) {
	if value == 0 { output.push(1); return; }
	let mut v2 = value << 1; // on positive number, lowest bit becomes 0, on negative -- 1
	if value < 0 { v2 = !v2; } // invert negative numbers. youngest bit is now 1 for positive
	let mut v3 = v2.abs() as u64; // conversion because we need unsigned value

	while v3 > 0 {
		let mut piece = (v3 & 127) as u8; // take lowest 7 bits
		v3 = v3 >> 7;
		if v3 > 0 { piece += 128; } // if it's not the last bit, set oldest bit to 1
		output.push(piece)
	}
}

fn decode_int(value: &[u8]) -> i64 {
	let mut result: i64 = 0;
	let mut sign = 1;
	for (shift, v) in value.iter().enumerate() {
		let v2 = v % 128;
		if shift == 0 && v % 2 == 0 { // in the youngest bit (the lowest in the 0th u8), 1 means the number is >=0
			sign = -1;
		}
		result |= (v2 as i64) << (shift * 7);
	}
	!result / 2 * sign
}


#[derive(Debug, Clone)]
pub enum CompLsError {
	EmptyLineString,
	BrokenLineString(String),
	BrokenEncoding(String),
}

impl Display for CompLsError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "{:?}", &self) }
}
impl Error for CompLsError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompLs {
	coords: Vec<u8>
}

impl CompLs {
	/// Create a new CompLs from bytes and check for consistency.
	pub fn try_new(coords: &[u8]) -> Result<Self, CompLsError> {
		let s: usize = coords.iter().filter(|v| **v < 128).count();
		if s & 1_usize == 0 { // check if the lowest bit is 0
			Ok(Self { coords: coords.into() })
		} else {
			Err(CompLsError::BrokenEncoding("number of coordinates in encoding is odd, must be even".into()))
		}
	}

	pub fn size(&self) -> usize {
		self.coords.iter().filter(|v| **v < 128).count() >> 1 // >>1 is integer division by 2
	}

	pub fn try_encode2(value: &LineString) -> Result<Self, CompLsError> {
		Self::try_encode(value, Precision::Two)
	}

	pub fn try_encode7(value: &LineString) -> Result<Self, CompLsError> {
		Self::try_encode(value, Precision::Seven)
	}

	pub fn try_encode(value: &LineString, precision: Precision) -> Result<Self, CompLsError> {
		let m = precision.multiplicator();

		let mut prev = &Coord { x: 0.0, y: 0.0 };
		let mut coords : Vec<u8> = vec![];
		for c in value.0.iter() {
			let Coord { mut x, mut y } = *c - *prev;
			if x.is_nan() || x.is_infinite() || y.is_nan() || y.is_infinite() { return Err(CompLsError::BrokenLineString("x or y coord is infinite".into())) }
			if x < 0.0 { x -= 1.0 / m; } // negative values must be lowered by 0.01 (or 1e-7 for 7-digit precision), because when converting back, negative values are -1,-2,.. and any tiny negative (in -1..0) becomes -1 (-0.01m), which adds noise.
			if y < 0.0 { y -= 1.0 / m; }
			encode_int((x * m).round() as i64, &mut coords);
			encode_int((y * m).round() as i64, &mut coords);
			prev = c;
		}
		Ok(Self { coords })
	}

	pub fn linestring(&self, precision: Precision) -> LineString {
		let multi = precision.multiplicator();
		// let capacity = (self.coords.len() - 6) >> 2;
		let mut ls = LineString(Vec::with_capacity(self.size()));
		let mut prev = Coord { x: 0.0, y: 0.0 };

		let mut i = 0_usize;
		let mut j = 0_usize;

		let mut coord_x: Option<f64> = None;
		while j < self.coords.len() {
			if self.coords[j] < 128 {
				let mut decoded = decode_int(&self.coords[i..=j]) as f64 / multi;
				if decoded < 0.0 { decoded += 1.0 / multi }

				if let Some(x) = coord_x {
					let c = Coord { x, y: decoded } + prev;
					coord_x = None;
					ls.0.push(c);
					prev = c;
				} else {
					coord_x = Some(decoded);
				}
				i = j + 1;
			}
			j += 1;
		}
		ls
	}

}

/// Convenience trait wrapping a function call. Allows instead of this:
///
///    CompLs::try_encode(&my_linestring)?
///
/// to write this:
///
///    my_linestring.try_compact()?
pub trait ToCompLs {
	fn try_compact(&self, precision: Precision) -> Result<CompLs, CompLsError>;
	fn try_compact2(&self) -> Result<CompLs, CompLsError>;
	fn try_compact7(&self) -> Result<CompLs, CompLsError>;
}

impl ToCompLs for LineString {
	fn try_compact2(&self) -> Result<CompLs, CompLsError> {
		CompLs::try_encode(self, Precision::Two)
	}
	fn try_compact7(&self) -> Result<CompLs, CompLsError> {
		CompLs::try_encode(self, Precision::Seven)
	}
	fn try_compact(&self, precision: Precision) -> Result<CompLs, CompLsError> {
		CompLs::try_encode(self, precision)
	}
}

pub mod compls_p2 {
	use serde::{Deserialize, Serializer, Deserializer, Serialize};
	use geo::LineString;
	use super::CompLs;

	pub fn serialize<S>(g: &LineString, serializer: S) -> Result<S::Ok, S::Error>
	where S: Serializer,
	{
		let s = CompLs::try_encode(g, crate::Precision::Two).map_err(serde::ser::Error::custom)?;
		// this code could work if CompLs had no Serde derived traits (Serialize, Deserialize)
		// but it's hard to implement Visitor struct, so I just use the derived methods.
		// if we want to put Serde in a feature, this should be done.
		/*let mut ser = serializer.serialize_struct("CompLs", 2)?;
		ser.serialize_field("first", &s.first)?;
		ser.serialize_field("deltas", &s.deltas)?;
		ser.end()*/
		s.serialize(serializer)
	}

	pub fn deserialize<'de, D>(deserializer: D) -> Result<LineString, D::Error>
	where D: Deserializer<'de>,
	{
		let lc = CompLs::deserialize(deserializer)?;
		// TODO: add error here if wrong number of coords
		Ok(lc.linestring(crate::Precision::Two))
	}
}

pub mod compls_p7 {
	/// Convenience
	use serde::{Deserialize, Serializer, Deserializer, Serialize};
	use geo::LineString;
	use super::CompLs;

	pub fn serialize<S>(g: &LineString, serializer: S) -> Result<S::Ok, S::Error>
	where S: Serializer
	{
		let s = CompLs::try_encode(g, crate::Precision::Seven).map_err(serde::ser::Error::custom)?;
		s.serialize(serializer)
	}

	pub fn deserialize<'de, D>(deserializer: D,) -> Result<LineString, D::Error>
	where D: Deserializer<'de>
	{
		let lc = CompLs::deserialize(deserializer)?;
		// TODO: add error here if wrong number of coords
		Ok(lc.linestring(crate::Precision::Seven))
	}
}


#[macro_export]
macro_rules! assert_ls_eq {
	($ls1:expr, $ls2:expr) => {
		{
			let g1 = $ls1;
			let g2 = $ls2;
			assert_eq!(g1.0.len(), g2.0.len());
			for i in 0..g1.0.len() {
				let c1 = g1.0[i];
				let c2 = g2.0[i];
				let Coord { x, y } = c2 - c1;
				assert!((x - 0.0).abs() <= 0.1);
				assert!((y - 0.0).abs() <= 0.1);
			}
		}
	};
}


#[macro_export]
macro_rules! wktls {
	($($x:literal $y:literal),+) => {
		geo::LineString(vec![$(geo::Coord { x: $x, y: $y }),+])
	}
}

#[cfg(test)]
mod compls_tests {
	use super::*;
	use bincode::{serialize, deserialize};


	#[test]
	fn serialize_and_length() {
		#[derive(Serialize, Deserialize)]
		struct SerializeTest {
			#[serde(with="compls_p7")]
			pub data: LineString
		}

		for l in [
			wktls!(76.9017028 43.1802978),
			wktls!(76.8936157 43.2443809,76.8936309 43.2442245),
			wktls!(76.8397903 43.2167510,76.8398132 43.2167587,76.8408584 43.2169990),
			wktls!(76.9756393 43.2715377,76.9760818 43.2720947,76.9766235 43.2728042),
			wktls!(76.9615707 43.2746200,76.9616699 43.2747688,76.9620742 43.2753715,76.9627532 43.2764091,76.9629516 43.2765502,76.9630584 43.2765998),
			wktls!(76.9759140 43.2704200,76.9757766 43.2705001,76.9756774 43.2705917,76.9755706 43.2707099,76.9754562 43.2708740,76.9753875 43.2710494,76.9754028 43.2711601,76.9754638 43.2713012,76.9756011 43.2714843,76.9756393 43.2715377),
		] {
			let length = l.0.len();
			let compln = l.try_compact7().unwrap();
			assert_eq!(length, compln.size());

			let item1 = SerializeTest { data: l.clone() };
			let data = serialize(&item1).unwrap();
			let item2: SerializeTest = deserialize(&data).unwrap();
			println!("{:?} <=> {:?}", l, item2.data);
			assert_ls_eq!(l, item2.data);
		}
	}
}