use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::io::BufWriter;
use std::path::Path;
use std::str::from_utf8_unchecked;

fn main() {
    let mut args = std::env::args();
    args.next();
    let original_path = args.next().expect("original_path");
    let modified_path = args.next().expect("modified_path");
    let original = load_yaml(&original_path);
    let modified = load_yaml(&modified_path);
    println!("{:#?}", &original);
    println!("{:#?}", &modified);
    if original.header != modified.header {
        panic!("header mismatch")
    }

    let mut modified_blocks = HashMap::<&BlockDescriptor, Vec<&Block>>::new();

    for x in &modified.blocks {
        let list = match modified_blocks.entry(&x.descriptor) {
            Entry::Occupied(o) => o.into_mut(),
            Entry::Vacant(v) => v.insert(Vec::with_capacity(1)),
        };
        list.push(x)
    }

    let mut id_mapping = HashMap::<u64, u64>::new();
    let mut blocks = Vec::with_capacity(original.blocks.len());

    for block in &original.blocks {
        let block_descriptor = &block.descriptor;
        let list = modified_blocks
            .get_mut(block_descriptor)
            .and_then(|v| if v.is_empty() { None } else { Some(v) })
            .ok_or_else(|| format!("no block found for {:?}", block_descriptor))
            .unwrap();
        let modified = list.remove(0);
        blocks.push(modified.clone());
        if let Some(_) = id_mapping.insert(modified.id, block.id) {
            panic!("id duplication: {}", modified.id);
        }
    }
    for block in blocks.iter_mut() {
        block.body = replace_ids(&block.body, &id_mapping);
        block.id = *id_mapping.get(&block.id).unwrap();
    }

    let created = PrefabFile {
        header: modified.header,
        blocks,
    };
    println!("{:#?}", &created);
    std::fs::rename(&modified_path, format!("{}.bak", &modified_path)).unwrap();
    let mut output = BufWriter::new(std::fs::File::create(&modified_path).unwrap());
    dump_yaml(&created, &mut output).unwrap();
}

fn load_yaml(path: impl AsRef<Path>) -> PrefabFile {
    parse_yaml(&std::fs::read_to_string(path).unwrap())
}

fn parse_yaml(file: &str) -> PrefabFile {
    let mut iter = split_yaml(file);
    let header = iter.next().unwrap().to_owned();
    let blocks = iter.collect::<Vec<_>>();
    // id -> name
    let mut object_names = HashMap::new();
    collect_game_objects(&blocks, &mut object_names);
    let blocks = blocks
        .iter()
        .map(|x| parse_block(x, &object_names))
        .collect();
    PrefabFile { header, blocks }
}

fn dump_yaml(file: &PrefabFile, writer: &mut impl std::io::Write) -> std::io::Result<()> {
    write!(writer, "{}", file.header)?;
    for block in &file.blocks {
        write!(writer, "{}", block.body)?;
    }
    Ok(())
}

fn collect_game_objects<'a>(body: &'_ Vec<&'a str>, objects: &'_ mut HashMap<u64, &'a str>) {
    for element in body {
        if let Some((id, name)) = find_game_object(&element.lines().collect()) {
            objects.insert(id, name);
        }
    }
}

fn find_game_object<'a>(lines: &'_ Vec<&'a str>) -> Option<(u64, &'a str)> {
    if lines[1].starts_with("GameObject:") {
        let id = lines[0].split_once('&').unwrap().1.trim().parse().unwrap();
        let name_line = lines
            .iter()
            .filter(|x| x.starts_with("  m_Name:"))
            .nth(0)
            .unwrap();
        let name = name_line.split_once(':').unwrap().1.trim();
        Some((id, name))
    } else {
        None
    }
}

fn parse_block(block: &str, object_names: &HashMap<u64, &str>) -> Block {
    let lines = block.lines().collect::<Vec<_>>();
    let id: u64 = lines[0].split_once('&').unwrap().1.trim().parse().unwrap();
    let descriptor = if lines[1].starts_with("GameObject:") {
        let name_line = lines
            .iter()
            .filter(|x| x.starts_with("  m_Name:"))
            .nth(0)
            .unwrap();
        let name = name_line.split_once(':').unwrap().1.trim();
        BlockDescriptor::GameObject(name.to_owned())
    } else {
        let game_object_line = lines
            .iter()
            .filter(|x| x.starts_with("  m_GameObject:"))
            .nth(0)
            .unwrap();
        let game_object_id: u64 = game_object_line
            .split_once(':')
            .unwrap()
            .1
            .split_once(':')
            .unwrap()
            .1
            .split_once('}')
            .unwrap()
            .0
            .trim()
            .parse()
            .unwrap();

        let name = object_names.get(&game_object_id).unwrap();

        BlockDescriptor::Others(lines[1].to_owned(), (*name).to_owned())
    };

    Block {
        body: block.to_owned(),
        id,
        descriptor,
    }
}

fn replace_ids(block: &str, id_mapping: &HashMap<u64, u64>) -> String {
    let mut is_in_number = false;
    let mut start_index = 0;
    let mut result = Vec::<u8>::with_capacity(block.len() + 10);
    let block = block.as_bytes();

    for (i, c) in block.iter().enumerate() {
        if is_in_number {
            if !matches!(*c, b'0'..=b'9') {
                if let Ok(value) =
                    unsafe { from_utf8_unchecked(block.get_unchecked(start_index..i)) }
                        .parse::<u64>()
                {
                    if let Some(mapped) = id_mapping.get(&value) {
                        result.extend_from_slice(mapped.to_string().as_bytes());
                        start_index = i;
                    }
                }
                is_in_number = false;
            }
        }
        if !is_in_number {
            if matches!(*c, b'0'..=b'9' | b'-') {
                result.extend_from_slice(
                    unsafe { from_utf8_unchecked(block.get_unchecked(start_index..i)) }.as_bytes(),
                );
                start_index = i;
                is_in_number = true;
            }
        }
    }

    result.extend_from_slice(
        unsafe { from_utf8_unchecked(block.get_unchecked(start_index..)) }.as_bytes(),
    );

    unsafe { String::from_utf8_unchecked(result) }
}

#[derive(Debug)]
struct PrefabFile {
    header: String,
    blocks: Vec<Block>,
}

#[derive(Debug, Clone)]
struct Block {
    body: String,
    id: u64,
    descriptor: BlockDescriptor,
}

#[derive(Debug, Hash, Eq, PartialEq, Clone)]
enum BlockDescriptor {
    GameObject(/*name*/ String),
    Others(/*type*/ String, /*of GameObject*/ String),
}

fn split_yaml(string: &str) -> SplitYaml {
    SplitYaml {
        haystack: string,
        index: 0,
    }
}

struct SplitYaml<'a> {
    haystack: &'a str,
    index: usize,
}

impl<'a> Iterator for SplitYaml<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index == self.haystack.len() {
            Option::None
        } else {
            let rest = unsafe { self.haystack.get_unchecked(self.index..) };
            let old_index = self.index;
            if let Some(index) = rest.find("\n---") {
                self.index += index + 1; // \n
                if self.haystack.as_bytes()[self.index - 1] != b'\n' {
                    panic!("no tailing new line")
                }
                unsafe { Option::Some(&self.haystack.get_unchecked(old_index..self.index)) }
            } else {
                self.index = self.haystack.len();
                if self.haystack.as_bytes()[self.index - 1] != b'\n' {
                    panic!("no tailing new line")
                }
                unsafe { Option::Some(&self.haystack.get_unchecked(old_index..self.index)) }
            }
        }
    }
}
