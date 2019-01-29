// flatkvm
// Copyright (C) 2019  Sergio Lopez <slp@sinrega.org>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <http://www.gnu.org/licenses/>.

use std::collections::HashMap;
use std::env;
use std::fs::{copy, create_dir_all, remove_file};
use std::path::Path;
use std::process::{exit, Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::thread;

use clap::{crate_authors, crate_version, App, Arg, SubCommand};
use dbus::arg::{RefArg, Variant};
use dbus::{BusType, Connection};
use log::{debug, error, info, log};
use x11_clipboard::{Clipboard, Source};

use flatkvm_qemu::agent::*;
use flatkvm_qemu::clipboard::*;
use flatkvm_qemu::dbus_codegen::*;
use flatkvm_qemu::dbus_notifications::DbusNotification;
use flatkvm_qemu::runner::{QemuRunner, QemuSharedDirType};

// TODO - This should be obtained from flatpak
const FLATPAK_SYSTEM_DIR: &str = "/var/lib/flatpak";
const FLATPAK_USER_DIR: &str = ".local/share/flatpak";
const FLATPAK_APP_DIR: &str = ".var/app";

// TODO - This should be configurable
const FLATKVM_APP_DIR: &str = ".var/flatkvm-app";
const FLATKVM_RUN_DIR: &str = ".var/run/flatkvm";
const DEFAULT_TEMPLATE: &str = "/usr/share/flatkvm/template-debian.qcow2";
const DEFAULT_TEMPLATE_DATA: &str = "/usr/share/flatkvm/template-debian-data.qcow2";

enum Message {
    LocalClipboardEvent(ClipboardEvent),
    RemoteClipboardEvent(ClipboardEvent),
    RemoteDbusNotification(DbusNotification),
    AppExit(i32),
    AgentClosed,
    QemuExit,
}

struct AgentListener {
    agent: AgentHost,
    sender: Sender<Message>,
}

impl AgentListener {
    pub fn new(agent: AgentHost, sender: Sender<Message>) -> AgentListener {
        AgentListener { agent, sender }
    }

    pub fn get_and_process_event(&mut self) -> bool {
        match self.agent.get_event().expect("Error listening for events") {
            AgentMessage::AgentAppExitCode(ec) => {
                debug!("Application exited on VM");
                self.sender.send(Message::AppExit(ec.code)).unwrap();
            }
            AgentMessage::ClipboardEvent(ce) => {
                debug!("VM sent a clipboard event");
                self.sender.send(Message::RemoteClipboardEvent(ce)).unwrap();
            }
            AgentMessage::DbusNotification(dn) => {
                debug!("VM sent a dbus notification");
                self.sender
                    .send(Message::RemoteDbusNotification(dn))
                    .unwrap();
            }
            AgentMessage::AgentClosed => {
                debug!("Connection to agent closed");
                self.sender.send(Message::AgentClosed).unwrap();
                return false;
            }
            _ => panic!("unexpected event"),
        }
        true
    }
}

fn verify_flatpak_app(app: &str, usermode: bool) -> Result<(), ()> {
    let mut args = vec!["info"];

    if usermode {
        args.push("--user");
    }

    args.push(app);

    let exit_status = Command::new("flatpak")
        .args(&args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|err| {
            error!("can't execute flatpak: {}", err.to_string());
            exit(-1);
        })?;

    let exit_code = match exit_status.code() {
        Some(code) => code,
        None => -1,
    };

    if exit_code == 0 {
        Ok(())
    } else {
        Err(())
    }
}

fn main() {
    env_logger::init();

    let cmd_args = App::new("flatkvm")
        .version(crate_version!())
        .author(crate_authors!())
        .about("Run a flatpak inside a KVM Guest.")
        .arg(
            Arg::with_name("user")
                .long("user")
                .help("Work on user installations"),
        )
        .subcommand(
            SubCommand::with_name("run")
                .about("Runs an already created flatpak KVM Guest")
                .arg(Arg::with_name("app").required(true))
                .arg(
                    Arg::with_name("user")
                        .long("user")
                        .help("Work on user installations"),
                )
                .arg(
                    Arg::with_name("cpus")
                        .long("cpus")
                        .short("c")
                        .takes_value(true)
                        .help("Number of vCPUs for the VM (default: 1)"),
                )
                .arg(
                    Arg::with_name("mem")
                        .long("mem")
                        .short("m")
                        .takes_value(true)
                        .help("Amount of RAM in MBs for the VM (default: 1024)"),
                )
                .arg(
                    Arg::with_name("no-shutdown")
                        .short("n")
                        .long("no-shutdown")
                        .help("Don't send shutdown signal when app exits"),
                )
                .arg(
                    Arg::with_name("no-audio")
                        .long("no-audio")
                        .help("Disable audio emulation"),
                )
                .arg(
                    Arg::with_name("no-network")
                        .long("no-network")
                        .help("Disable network emulation"),
                )
                .arg(
                    Arg::with_name("virgl")
                        .long("virgl")
                        .help("Enable Virgl 3D acceleration"),
                )
                .arg(
                    Arg::with_name("no-clipboard")
                        .long("no-clipboard")
                        .help("Disable automatic clipboard sharing"),
                )
                .arg(
                    Arg::with_name("no-dbus-notifications")
                        .long("no-dbus-notifications")
                        .help("Disable D-Bus notifications"),
                )
                .arg(
                    Arg::with_name("volatile")
                        .long("volatile")
                        .short("v")
                        .help("Use a temporary location for app data"),
                ),
        )
        .get_matches();
    let home_dir = env::var("HOME").unwrap();
    let flatpak_user_dir = format!("{}/{}", home_dir, FLATPAK_USER_DIR);
    let flatkvm_run_dir = format!("{}/{}", home_dir, FLATKVM_RUN_DIR);
    if !Path::new(&flatkvm_run_dir).exists() {
        create_dir_all(&flatkvm_run_dir).expect("can't create flatkvm run dir");
    }

    if let Some(run_args) = cmd_args.subcommand_matches("run") {
        let usermode = cmd_args.is_present("user") || run_args.is_present("user");
        let no_clipboard: bool = run_args.is_present("no-clipboard");
        let no_shutdown: bool = run_args.is_present("no-shutdown");
        let no_dbus_notifications: bool = run_args.is_present("no-dbus-notifications");
        let appname = run_args.value_of("app").expect("missing argument");
        let app: &str = match appname.find("/") {
            Some(i) => &appname[..i],
            None => appname,
        };

        match verify_flatpak_app(app, usermode) {
            Ok(_) => (),
            Err(_) => {
                error!("can't find flatpak app: {}", app);
                exit(-1);
            }
        }

        let flatpak_app_dir = format!("{}/{}/{}", home_dir, FLATPAK_APP_DIR, app);
        if !Path::new(&flatpak_app_dir).exists() {
            create_dir_all(&flatpak_app_dir).expect("can't create flatkvm app dir");
        }
        let flatkvm_app_dir = format!("{}/{}/{}", home_dir, FLATKVM_APP_DIR, app);
        if !Path::new(&flatkvm_app_dir).exists() {
            create_dir_all(&flatkvm_app_dir).expect("can't create flatkvm app dir");
        }

        let agent_sock_path = format!("{}/{}-agent.sock", flatkvm_run_dir, app);
        let qmp_sock_path = format!("{}/{}-qmp.sock", flatkvm_run_dir, app);

        let cpus: u32 = match run_args.value_of("cpus") {
            Some(cpus) => cpus.parse().unwrap(),
            None => 1,
        };
        let mem: u32 = match run_args.value_of("mem") {
            Some(mem) => mem.parse().unwrap(),
            None => 1024,
        };

        let data_disk = format!("{}/{}-disk.qcow2", flatkvm_app_dir, app);
        if !Path::new(&data_disk).exists() {
            copy(DEFAULT_TEMPLATE_DATA, &data_disk).expect("can't copy data template");
        }

        let mut qemu_runner = QemuRunner::new(app.to_string(), data_disk)
            .vcpu_num(cpus)
            .ram_mb(mem)
            .template(DEFAULT_TEMPLATE.to_string())
            .agent_sock_path(agent_sock_path.clone())
            .qmp_sock_path(qmp_sock_path)
            .shared_dir(
                QemuSharedDirType::FlatpakSystemDir,
                FLATPAK_SYSTEM_DIR.to_string(),
                true,
            )
            .shared_dir(
                QemuSharedDirType::FlatpakUserDir,
                flatpak_user_dir.to_string(),
                true,
            );

        if run_args.is_present("volatile") {
            qemu_runner = qemu_runner.volatile(true);
        }
        if run_args.is_present("no-audio") {
            qemu_runner = qemu_runner.audio(false);
        }
        if run_args.is_present("no-network") {
            qemu_runner = qemu_runner.network(false);
        }
        if run_args.is_present("virgl") {
            qemu_runner = qemu_runner.virgl(true);
        }

        let mut qemu_child = qemu_runner.run().expect("can't start QEMU instance");

        debug!("Opening QMP connection...");
        let qmp_conn = qemu_runner
            .get_qmp_conn()
            .expect("can't open QMP connection");
        qmp_conn
            .initialize()
            .expect("error initializing QMP connection");

        debug!("Waiting for agent...");
        let mut agent = qemu_runner
            .get_agent()
            .expect("can't open agent connection");
        let agent_ready = agent
            .initialize()
            .expect("error initalizing agent connection");

        info!("Agent connected, version: {}", agent_ready.version);
        remove_file(agent_sock_path)
            .map_err(|err| debug!("can't remove file: {}", err.to_string()))
            .unwrap();

        debug!("Sending commands to agent");
        for dir in qemu_runner.get_shared_dirs() {
            debug!("Requesting mount for shared_dir: {:?}", dir);
            agent.request_mount(dir).expect("error mounting 9p fs");
        }
        agent
            .request_run(app.to_string(), usermode, !no_dbus_notifications)
            .expect("error running app");

        // Create a specific channel for clipboard messages.
        let (clipboard_sender, clipboard_receiver) = channel();

        // Spawn a thread to listen for clipboard events.
        let cb_used_flag = Arc::new(AtomicBool::new(false));
        if !no_clipboard {
            // TODO - If enabled, we share data push into the clipboard
            // with the VM. Ideally, there should be a dbus service
            // listenting for an event generated from some kind of UI.
            ClipboardListener::new(clipboard_sender.clone(), cb_used_flag.clone()).spawn_thread();
        }

        // Create a channel for general thread coordination.
        let (common_sender, common_receiver) = channel();

        // Translate clipboard messages into our own kind.
        let cb_sender = common_sender.clone();
        thread::spawn(move || loop {
            for msg in &clipboard_receiver {
                match msg {
                    ClipboardMessage::ClipboardEvent(ce) => {
                        cb_sender.send(Message::LocalClipboardEvent(ce)).unwrap();
                    }
                }
            }
        });

        // Spawn a thread waiting for messages coming from the Guest.
        let mut agent_listener =
            AgentListener::new(agent.try_clone().unwrap(), common_sender.clone());
        thread::spawn(move || loop {
            if !agent_listener.get_and_process_event() {
                break;
            };
        });

        // Spawn a thread to wait for QEMU process to exit.
        let exit_sender = common_sender.clone();
        thread::spawn(move || {
            debug!("Waiting for QEMU process to finish");
            qemu_child.wait().expect("QEMU process wasn't running");
            exit_sender.send(Message::QemuExit).unwrap();
            debug!("QEMU process finished");
        });

        // Create a D-Bus connection to relay VM notifications.
        let dbus_conn = Connection::get_private(BusType::Session).unwrap();
        let dbus_conn_path = dbus_conn.with_path(
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            5000,
        );

        // Process events coming from spawned threads.
        let clipboard = Clipboard::new(Source::Clipboard).unwrap();
        for msg in common_receiver {
            match msg {
                Message::LocalClipboardEvent(ce) => {
                    debug!("Clipboard event: {}", ce.data);
                    agent.send_clipboard_event(ce.data).unwrap();
                }
                Message::RemoteClipboardEvent(ce) => {
                    debug!("RemoteClipboard: {}", ce.data);
                    if !no_clipboard {
                        cb_used_flag.store(true, Ordering::Relaxed);
                        clipboard
                            .store(
                                clipboard.setter.atoms.clipboard,
                                clipboard.setter.atoms.utf8_string,
                                ce.data.as_bytes(),
                            )
                            .expect("clipboard.store");
                    }
                }
                Message::RemoteDbusNotification(dn) => {
                    debug!("RemoteDbusNotification");
                    if !no_dbus_notifications {
                        // Notifications should have been suppresed from the VM-side,
                        // but we check it again to be extra-sure.
                        let actions: Vec<&str> = Vec::new();
                        let hints: HashMap<&str, Variant<Box<RefArg>>> = HashMap::new();

                        dbus_conn_path
                            .notify(
                                app,
                                0,
                                "",
                                &dn.summary,
                                &dn.body,
                                actions,
                                hints,
                                dn.expire_timeout,
                            )
                            .unwrap();
                    }
                }
                Message::AppExit(ec) => {
                    debug!("AppExit with error code: {}", ec);
                    if !no_shutdown {
                        qmp_conn
                            .send_shutdown()
                            .map_err(|err| debug!("error sending shutdown: {}", err.to_string()))
                            .unwrap();
                    }
                }
                Message::AgentClosed => {
                    debug!("The agent has closed the connection");
                }
                Message::QemuExit => {
                    debug!("QEMU has shut down");
                    break;
                }
            }
        }
    }

    debug!("Ending!");
}
