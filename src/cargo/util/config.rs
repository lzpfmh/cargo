extern crate collections;
extern crate serialize;
extern crate toml;

use super::super::{CargoResult,ToCargoError,CargoError};
use serialize::{Encodable,Encoder};
use std::{io,fmt};

#[deriving(Eq,TotalEq,Clone,Encodable,Decodable)]
pub enum Location {
    Project,
    Global
}

#[deriving(Eq,TotalEq,Clone,Decodable)]
pub enum ConfigValueValue {
    String(~str),
    List(Vec<~str>)
}

impl fmt::Show for ConfigValueValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            &String(ref string) => write!(f.buf, "{}", string),
            &List(ref list) => write!(f.buf, "{}", list)
        }
    }
}

impl<E, S: Encoder<E>> Encodable<S, E> for ConfigValueValue {
    fn encode(&self, s: &mut S) -> Result<(), E> {
        match self {
            &String(ref string) => {
                try!(string.encode(s));
            },
            &List(ref list) => {
                try!(list.encode(s));
            }
        }

        Ok(())
    }
}

#[deriving(Eq,TotalEq,Clone,Decodable)]
pub struct ConfigValue {
    value: ConfigValueValue,
    path: Vec<~str>
}

impl ConfigValue {
    pub fn new() -> ConfigValue {
        ConfigValue { value: List(vec!()), path: vec!() }
    }

    pub fn get_value<'a>(&'a self) -> &'a ConfigValueValue {
        &self.value
    }
}

impl<E, S: Encoder<E>> Encodable<S, E> for ConfigValue {
    fn encode(&self, s: &mut S) -> Result<(), E> {
        s.emit_map(2, |s| {
            try!(s.emit_map_elt_key(0, |s| "value".encode(s)));
            try!(s.emit_map_elt_val(0, |s| self.value.encode(s)));
            Ok(())
        })
    }
}

impl fmt::Show for ConfigValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f.buf, "{} (from {})", self.value, self.path)
    }
}

pub fn get_config(pwd: Path, key: &str) -> CargoResult<ConfigValue> {
    find_in_tree(&pwd, |file| extract_config(file, key)).to_cargo_error(format!("Config key not found: {}", key), 1)
}

pub fn all_configs(pwd: Path) -> CargoResult<collections::HashMap<~str, ConfigValue>> {
    let mut map = collections::HashMap::new();

    try!(walk_tree(&pwd, |file| {
        extract_all_configs(file, &mut map)
    }));

    Ok(map)
}

#[allow(unused_variable)]
pub fn set_config(key: ~str, value: ~str, location: Location) -> CargoResult<()> {
    Ok(())
}

fn find_in_tree<T>(pwd: &Path, walk: |io::fs::File| -> CargoResult<T>) -> CargoResult<T> {
    let mut current = pwd.clone();

    loop {
        let possible = current.join(".cargo").join("config");
        if possible.exists() {
            let file = try!(io::fs::File::open(&possible).to_cargo_error(~"", 1));
            match walk(file) {
                Ok(res) => return Ok(res),
                _ => ()
            }
        }

        if !current.pop() { break; }
    }

    Err(CargoError::new(~"", 1))
}

fn walk_tree(pwd: &Path, walk: |io::fs::File| -> CargoResult<()>) -> CargoResult<()> {
    let mut current = pwd.clone();
    let mut err = false;

    loop {
        let possible = current.join(".cargo").join("config");
        if possible.exists() {
            let file = try!(io::fs::File::open(&possible).to_cargo_error(~"", 1));
            match walk(file) {
                Err(_) => err = false,
                _ => ()
            }
        }

        if err { return Err(CargoError::new(~"", 1)); }
        if !current.pop() { break; }
    }

    Ok(())
}

fn extract_config(file: io::fs::File, key: &str) -> CargoResult<ConfigValue> {
    let path = try!(file.path().as_str().to_cargo_error(~"", 1)).to_owned();
    let mut buf = io::BufferedReader::new(file);
    let root = try!(toml::parse_from_buffer(&mut buf).to_cargo_error(~"", 1));
    let val = try!(root.lookup(key).to_cargo_error(~"", 1));

    let v = match val {
        &toml::String(ref val) => String(val.to_owned()),
        &toml::Array(ref val) => List(val.iter().map(|s: &toml::Value| s.to_str()).collect()),
        _ => return Err(CargoError::new(~"", 1))
    };

    Ok(ConfigValue{ value: v, path: vec!(path) })
}

fn extract_all_configs(file: io::fs::File, map: &mut collections::HashMap<~str, ConfigValue>) -> CargoResult<()> {
    let path = try!(file.path().as_str().to_cargo_error(~"", 1)).to_owned();
    let mut buf = io::BufferedReader::new(file);
    let root = try!(toml::parse_from_buffer(&mut buf).to_cargo_error(~"", 1));
    let table = try!(root.get_table().to_cargo_error(~"", 1));

    for (key, value) in table.iter() {
        match value {
            &toml::String(ref val) => { map.insert(key.to_owned(), ConfigValue { value: String(val.to_owned()), path: vec!(path.clone()) }); }
            &toml::Array(ref val) => {
                let config = map.find_or_insert_with(key.to_owned(), |_| {
                    ConfigValue { path: vec!(), value: List(vec!()) }
                });

                try!(merge_array(config, val.as_slice(), path.as_slice()));
            },
            _ => ()
        }
    }

    Ok(())
}

fn merge_array(existing: &mut ConfigValue, val: &[toml::Value], path: &str) -> CargoResult<()> {
    match existing.value {
        String(_) => return Err(CargoError::new(~"", 1)),
        List(ref mut list) => {
            let new_list: Vec<CargoResult<~str>> = val.iter().map(|s: &toml::Value| toml_string(s)).collect();
            if new_list.iter().any(|v| v.is_err()) {
                return Err(CargoError::new(~"", 1));
            } else {
                let new_list: Vec<~str> = new_list.move_iter().map(|v| v.unwrap()).collect();
                list.push_all(new_list.as_slice());
                existing.path.push(path.to_owned());
                Ok(())
            }
        }
    }
}

fn toml_string(val: &toml::Value) -> CargoResult<~str> {
    match val {
        &toml::String(ref str) => Ok(str.to_owned()),
        _ => Err(CargoError::new(~"", 1))
    }
}