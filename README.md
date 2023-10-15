# CompLs, compact line string

The crate provides lossy compression of GeoRust's [`LineString`](https://docs.rs/geo/latest/geo/geometry/struct.LineString.html)s, via a structure and (de)serializer.

`LineString`s contain coordinates as pairs of `f64`s, hence linestring of N points requires 16\*N bytes. For example, in OSM, a city of 2 mln people, *not* in great detail, contains 27K ways with 215K coordinates (~7.96 coords per linestring). This would take 3.44Mb of storage or RAM.

In this crate, structure `CompLs` stores coordinates as deltas in integer [LEB128](https://en.wikipedia.org/wiki/LEB128) encoding, and instead of 16\*N bytes for `LineString`, `CompLs` uses just 6 + 4\*N bytes. The city mentioned above will need 27K \* (6 + 4 \* 7.96) = ~1.02 Mb for storage, or roughly 3.5 times less.

# Usage

## Encoding a LineString and decoding back

```rust
use cmpls::{CompLs, ToCompLs, wktls, assert_ls_eq};

// a handy macro to create linestring like WKT
let ls = wktls!(76.9615707 43.2746200,76.9616699 43.2747688,76.9620742 43.2753715,76.9627532 43.2764091,76.9629516 43.2765502,76.9630584 43.2765998);

// try compacting with 7-digit precision
let cmp = ls.try_compact7()?;

println!("{cmp:?}");

assert_ls_eq!(ls, cmp.linestring());
```

## Serializing normal LineStrings

You may decide to keep LineStrings in memory, but store them in compact format. The only thing you need is to specify `#[serde(with=...)]` attribute to the field, the rest is done under the hood.

```rust
use serde::{Serialize, Deserialize};
use cmpls::{compls_p2, compls_p7};

#[derive(Serialize, Deserialize)]
struct MyStruct {
	id: usize,
	#[serde(with="compls_p7")] // geometry in lat/lon need 7 digits of precision, hence compls_p7
	geometry_4326: LineString,
	#[serde(with="compls_p2")]  // geometries in metre-based CRS need just 2 digits, use compls_p2
	geometry_3857: LineString
}
```
