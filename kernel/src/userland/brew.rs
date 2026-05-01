use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use alloc::collections::BTreeMap;
use spin::Mutex;

static REPOS: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());

pub struct Package {
    pub name: String,
    pub version: String,
    pub description: String,
    pub url: String,
    pub sha256: String,
    pub dependencies: Vec<String>,
}

pub fn init() {
    let mut repos = REPOS.lock();
    repos.insert(String::from("core"), String::from("https://brew.sh/repos/core"));
    repos.insert(String::from("extra"), String::from("https://brew.sh/repos/extra"));
}

pub fn add_repo(name: &str, url: &str) {
    REPOS.lock().insert(String::from(name), String::from(url));
}

pub fn list_repos() -> Vec<(String, String)> {
    REPOS.lock().iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

pub fn install(name: &str) -> Result<(), &str> {
    log::info!("brew.sh: installing '{}'", name);
    if name.is_empty() { return Err("empty package name"); }
    Err("not yet implemented: network stack needed")
}

pub fn uninstall(name: &str) -> Result<(), &str> {
    log::info!("brew.sh: uninstalling '{}'", name);
    Err("not yet implemented")
}

pub fn list_installed() -> Vec<String> {
    Vec::new()
}

pub fn search(query: &str) -> Vec<String> {
    vec![format!("{}-1.0", query), format!("{}-utils", query)]
}
