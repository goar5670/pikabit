
use std::fmt::Debug;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn expect_eq<T: Eq + Debug>(received: T, expected: T) -> Result<()> {
    if received != expected {
        return Err(format!("Error verifying values, expected {:?}, recieved {:?}", expected, received).into());
    }

    Ok(())
}
