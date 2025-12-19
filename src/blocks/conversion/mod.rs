mod base;
mod bitfield;
mod formula;
mod linear;
mod logic;
mod table_lookup;
mod text;
mod types;

pub use base::ConversionBlock;
pub use types::ConversionType;

#[cfg(test)]
mod test_deep_chains;

#[cfg(test)]
mod simple_test;
