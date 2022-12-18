use std::time::SystemTime;

pub fn get_timestamp() -> String {
  let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH);
  let timestamp: String = now.unwrap().as_secs().to_string();

  timestamp
}
