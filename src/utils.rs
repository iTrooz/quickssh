use std::env;

use anyhow::Context;

pub fn get_username() -> anyhow::Result<String> {
    env::var("USER").context("Failed to read USER env variable")
}
