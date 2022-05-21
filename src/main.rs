use i3_ipc::event::{Event, Subscribe, WorkspaceChange};
use i3_ipc::reply::{Node, Workspace};
use i3_ipc::{Connect, I3Stream, I3};
use indoc::{formatdoc, indoc};
use std::borrow::{Borrow};
use std::collections::{BTreeMap, HashMap};
use std::{env, io, process};
use std::cell::RefCell;

const BOX: &str = indoc! {"
    (box :class 'i3wm-workspaces'
         :orientation 'h'
         :spacing 5
         :space-evenly 'false'
"};

fn get_button(num: &usize, name: &str, vis: &str) -> String {
    return formatdoc! {"
        (button   :class 'i3wm-workspace-{vis}'
                  :onclick 'i3-msg -t run_command workspace {num}'
                  '{name}')",
    num = num,
    vis = vis,
    name = name};
}

fn main() -> io::Result<()> {
    let mut map: BTreeMap<usize, String> = BTreeMap::new();
    let mut i3 = I3::connect()?;
    let monitor = match env::args().nth(1) {
        Some(m) => m,
        None => {
            println!("No monitor specified.");
            process::exit(1)
        }
    };

    print_initial(&mut i3, &mut map, &monitor);

    let mut listener = I3Stream::conn_sub(&[Subscribe::Workspace]).unwrap();
    for res in listener.listen() {
        let mut update = false;
        match res.unwrap() {
            Event::Workspace(e) => {
                match e.change {
                    WorkspaceChange::Urgent => {
                        let workspace = e.current.unwrap();
                        let (key, name) = get_name_key_from_node(&workspace).unwrap();
                        map.insert(key, get_button(&key, &name, &"urgent".to_string()));
                        update = true;
                    }
                    WorkspaceChange::Empty => {
                        // Workspace is dropped
                        let workspace = e.current.unwrap();
                        let key = workspace.name.unwrap().parse::<usize>().unwrap();
                        map.remove(&key);
                        update = true;
                    }
                    WorkspaceChange::Focus => {
                        // Focused a new workspace, may also call init or empty
                        let mut workspace = e.old.unwrap();
                        let (mut key, mut name) = get_name_key_from_node(&workspace).unwrap();

                        if map.contains_key(&key) {
                            match i3
                                .get_workspaces()?
                                .iter()
                                .find(|w| &w.name == workspace.name.as_ref().unwrap())
                            {
                                Some(_) => {
                                    map.insert(
                                        key,
                                        get_button(
                                            &key,
                                            &name,
                                            &get_visibility_node(&mut i3, &workspace),
                                        ),
                                    );
                                }
                                None => {
                                    map.remove(&key);
                                }
                            }
                            update = true;
                        }

                        workspace = e.current.unwrap();
                        (key, name) = get_name_key_from_node(&workspace).unwrap().clone();

                        if map.contains_key(&key) {
                            map.insert(key, get_button(&key, &name, &"focused".to_string()));
                            update = true;
                        }
                    }
                    WorkspaceChange::Init => {
                        // New workspace created
                        let workspace = e.current.unwrap();
                        let (key, name) = get_name_key_from_node(&workspace).unwrap();
                        map.insert(
                            key,
                            get_button(&key, &name, &get_visibility_node(&mut i3, &workspace)),
                        );
                    }
                    WorkspaceChange::Move => {
                        // Move output
                        let workspace = e.current.unwrap();
                        let output = &workspace.output;
                        let pair = get_name_key_from_node(&workspace).unwrap();

                        match output {
                            Some(ref o) => {
                                if o == &monitor && !map.contains_key(&pair.0) {
                                    map.insert(
                                        pair.0,
                                        get_button(
                                            &pair.0,
                                            &pair.1,
                                            &get_visibility_node(&mut i3, &workspace),
                                        ),
                                    );
                                    update = true;
                                } else if o != &monitor && map.contains_key(&pair.0) {
                                    map.remove(&pair.0);
                                    update = true;
                                }
                            }
                            _ => {
                                update = map.remove(&pair.0).is_some();
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => unreachable!(),
        }
        if update {
            print_workspaces(&map);
        }
    }
    Ok(())
}

thread_local! {
    static NAME_KEY: RefCell<HashMap<usize, (usize, String)>> = RefCell::new(HashMap::new());
}

fn get_name_key_from_workspace(workspace: &Workspace) -> Option<(usize, String)> {
    get_name_key(&workspace.id, &workspace.name)
}

fn get_name_key_from_node(node: &Node) -> Option<(usize, String)> {
    get_name_key(&node.id, &node.name.as_ref()?)
}

fn get_name_key<'a>(id: &'a usize, name: &'a str) -> Option<(usize, String)> {
    NAME_KEY.with(|r| {
        let mut map = r.borrow_mut();
        if !map.contains_key(id) {
            map.insert(
                *id,
                name.split_once(";").map_or_else(
                    || (name.parse::<usize>().unwrap(), name.to_string()),
                    |(num, s_name)| {
                        let mut name = s_name.to_string();
                        name.retain(|c| !c.is_ascii());
                        if name.len() == 0 {
                            name.push('');
                        }
                        (num.parse().unwrap(), name)
                    },
                ),
            );
        };
        return map.get(id).cloned();
    })
}

fn print_initial(i3: &mut I3Stream, map: &mut BTreeMap<usize, String>, monitor: &str) {
    for workspace in i3.get_workspaces().unwrap() {
        if workspace.output != monitor {
            continue;
        };

        let (key, name) = match get_name_key_from_workspace(&workspace) {
            None => continue,
            Some(w) => w,
        };

        map.insert(key, get_button(&key, &name, &get_visibility_workspace(&workspace)));
    }
    print_workspaces(map);
}

fn get_visibility_node(i3: &mut I3Stream, node: &Node) -> String {
    match i3
        .get_workspaces()
        .unwrap()
        .iter()
        .find(|w| w.id == node.id)
    {
        Some(w) => get_visibility_workspace(w),
        None => "".to_string(),
    }
}

fn get_visibility_workspace(workspace: &Workspace) -> String {
    if workspace.focused {
        "focused"
    } else if workspace.urgent {
        "urgent"
    } else if workspace.visible {
        "visible"
    } else {
        "hidden"
    }
    .to_string()
}

fn print_workspaces(map: &BTreeMap<usize, String>) {
    let mut string = formatdoc! {"
    {box}
    {buttons})
    ",
    box = BOX,
    buttons = map.iter().map(|(_, v)| v.borrow()).collect::<Vec<_>>().join("") };
    trim_newlines(&mut string);
    println!("{}", string);
}

fn trim_newlines(input: &mut String) {
    input.retain(|c| c != '\n');
}
