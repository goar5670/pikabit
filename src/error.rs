use anyhow;
use std::fmt::Debug;

pub fn expect_eq<T: Eq + Debug>(received: T, expected: T, context: &str) -> anyhow::Result<()> {
    if received != expected {
        anyhow::bail!(
            "Error verifying values in {}, expected {:?}, recieved {:?}",
            context,
            expected,
            received
        );
    }

    Ok(())
}
