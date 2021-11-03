mod dumper;
mod ipc;

use crate::dumper::dump;
use crate::ipc::{IPC, Connection};
use polodb_core::Database;
use polodb_core::msg_ty::MsgTy;
use clap::{Arg, App};
use std::process::exit;
use std::io::{Read, Write};
use std::sync::Arc;
use std::thread;
use std::sync::Mutex;
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use error_chain::error_chain;
use signal_hook::{iterator::Signals};
use signal_hook::consts::TERM_SIGNALS;
use std::path::{Path, PathBuf};

error_chain! {

    foreign_links {
        Bson( polodb_bson::BsonErr);
        Db( polodb_core::DbErr);
        Fmt(::std::fmt::Error);
        Io(::std::io::Error);
    }

    errors {
        UnwrapDocument {
            display("unwrap db failed")
        }
        UnwrapFail(str: String) {
            display("unwrap value failed: {}", str)
        }
    }

}

const HEAD: [u8; 4] = [0xFF, 0x00, 0xAA, 0xBB];
const PING_HEAD: [u8; 4] = [0xFF, 0x00, 0xAA, 0xCC];

#[derive(Clone)]
struct AppContext {
    socket_path: String,
    db: Arc<Mutex<Option<Database>>>,
}

impl AppContext {

    fn new(socket_path: String, db: Database) -> AppContext {
        AppContext {
            socket_path,
            db: Arc::new(Mutex::new(Some(db))),
        }
    }

    fn handle_incoming_connection(&self, conn: &mut Connection) -> Result<bool> {
        let mut db_guard = self.db.lock().unwrap();
        let db = db_guard.as_mut().unwrap();
        let mut header_buffer = [0u8; 4];

        conn.read_exact(&mut header_buffer)?;

        if header_buffer != HEAD {
            if header_buffer == PING_HEAD {
                let req_id = conn.read_u32::<BigEndian>()?;
                conn.write(&PING_HEAD)?;
                conn.write_u32::<BigEndian>(req_id)?;
                return Ok(true);
            }
            eprintln!("head is not matched, exit...");
            return Ok(false)
        }

        let req_id = conn.read_u32::<BigEndian>()?;

        let mut ret_buffer = Vec::new();

        let msg_ty_result = db.handle_request(conn, &mut ret_buffer);
        if let Err(err) = msg_ty_result {
            eprintln!("io error, exit: {}", err);
            return Ok(false);
        }

        let msg_ty = msg_ty_result.unwrap();

        conn.write(&HEAD)?;
        conn.write_u32::<BigEndian>(req_id)?;
        conn.write(&ret_buffer)?;
        conn.flush()?;

        if msg_ty == MsgTy::SafelyQuit {
            return Ok(false);
        }

        Ok(true)
    }

}

fn start_socket_server(path: Option<&str>, socket_addr: &str) {
    let db = match path {
        Some(path) => {
            match Database::open_file(path) {
                Ok(db) => db,
                Err(err) => {
                    eprintln!("open db {} failed: {}", path, err);
                    exit(6);
                }
            }
        },
        None => {
            match Database::open_memory() {
                Ok(db) => db,
                Err(err) => {
                    eprintln!("open memory db failed: {}", err);
                    exit(6);
                }
            }
        }
    };

    let app = AppContext::new(socket_addr.into(), db);

    start_app_async(app.clone(), socket_addr);

    let mut signals = Signals::new(TERM_SIGNALS).unwrap();
    for _ in signals.forever() {
        eprintln!("Received quit signal, prepare to exit");
        safely_quit(app.clone());
    }
}

fn start_app_async(app: AppContext, socket_addr: &str) {
    let socket_attr_copy: String = socket_addr.into();
    thread::spawn(move || {
        let listener = IPC::bind(socket_attr_copy).unwrap();

        for stream in listener.incoming() {
            let stream = stream.unwrap();

            eprintln!("Connection established!");
            let app = app.clone();
            thread::spawn(move || {
                let mut moved_stream = stream;
                loop {
                    let result = app.handle_incoming_connection(&mut moved_stream);
                    match result {
                        Ok(true) => {
                            continue;
                        },

                        Ok(false) => {
                            safely_quit(app.clone());
                        },

                        Err(err) => {
                            match err.0 {
                                ErrorKind::Io(_) => {
                                    eprintln!("io error: {}", err);
                                    return;
                                }
                                _ => {
                                    eprintln!("other error, continue: {}", err);
                                }
                            }
                        }
                    }
                }
            });
        }
    });
}

fn safely_quit(app: AppContext) {
    let mut db_guard = app.db.lock().unwrap();
    *db_guard = None;
    let _ = std::fs::remove_file(&app.socket_path);
    eprintln!("safely exit");
    exit(0);
}

fn migrate_v1_to_v2(path: &Path) {
    let result = Database::v1_to_v2(path);
    if let Err(err) = result {
        println!("{}", err);
        exit(8);
    }
}

fn main() {
    let version = Database::get_version();
    let app = App::new("PoloDB")
        .version(version.as_str())
        .about("Command line tool for PoloDB")
        .author("Vincent Chan <okcdz@diverse.space>")
        .subcommand(App::new("dump")
            .about("dump the database to text")
            .arg(
                Arg::with_name("path")
                    .index(1)
                    .required(true)
            )
            .arg(Arg::with_name("detail").required(false)))
        .subcommand(App::new("serve")
            .about("attach the database, start the tcp server")
            .arg(
                Arg::with_name("socket")
                    .short("s")
                    .long("socket")
                    .help("the domain socket to listen on").required(true)
                    .takes_value(true)
            )
            .arg(
                Arg::with_name("path")
                    .short("p")
                    .long("path")
                    .value_name("PATH")
                    .takes_value(true)
            )
            .arg(Arg::with_name("memory"))
            .arg(
                Arg::with_name("log")
                    .help("print log")
                    .long("log")
                    .short("l")
            )
        )
        .subcommand(App::new("migrate")
            .about("migrate the older database to the newer format")
            .arg(
                Arg::with_name("path")
                    .index(1)
                    .required(true)
            )
            .arg(
                Arg::with_name("target")
                    .long("target")
                    .value_name("TARGET")
                    .takes_value(true)
                    .required(true)
            )
        )
        .arg(
            Arg::with_name("log")
                .help("print log")
                .long("log")
                .short("l")
        );

    let matches = app.get_matches();

    if let Some(sub) = matches.subcommand_matches("serve") {
        let should_log = sub.is_present("log");
        Database::set_log(should_log);

        let socket = sub.value_of("socket").unwrap();
        let path = sub.value_of("path");
        if let Some(path) = path {
            start_socket_server(Some(path), socket);
        } else if sub.is_present("memory") {
            start_socket_server(None, socket);
        } else {
            eprintln!("you should pass either --path or --memory");
        }
        return;
    }

    if let Some(sub) = matches.subcommand_matches("dump") {
        let path = sub.value_of("path").expect("no input path");
        let detail = sub.is_present("detail");
        dump(path, detail);
        return;
    }

    if let Some(sub) = matches.subcommand_matches("migrate") {
        let target = sub.value_of("target").expect("target version not found");
        if target != "v2" {
            println!("only v2 format is supported, but {} got", target);
            exit(7);
        }
        let path_str = sub.value_of("path").expect("path not found");
        let path = PathBuf::from(path_str);
        migrate_v1_to_v2(&path);
        return;
    }

    println!("{}", matches.usage());
}
