mod error;
mod parsing;
mod plugin;
mod types;
mod util;

use types::QuillPlugin;
use zellij_tile::prelude::*;
#[cfg(target_arch = "wasm32")]
use {
    std::cell::RefCell,
    std::collections::BTreeMap,
    std::convert::{TryFrom, TryInto},
    zellij_tile::shim::plugin_api::action::ProtobufPluginConfiguration,
    zellij_tile::shim::plugin_api::event::ProtobufEvent,
    zellij_tile::shim::plugin_api::pipe_message::ProtobufPipeMessage,
    zellij_tile::shim::prost::Message,
};

#[cfg(target_arch = "wasm32")]
thread_local! {
    static STATE: RefCell<QuillPlugin> = RefCell::new(Default::default());
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn _start() {
    std::panic::set_hook(Box::new(|info| {
        report_panic(info);
    }));
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn load() {
    STATE.with(|state| {
        let protobuf_bytes: Vec<u8> = zellij_tile::shim::object_from_stdin().unwrap();
        let protobuf_configuration =
            ProtobufPluginConfiguration::decode(protobuf_bytes.as_slice()).unwrap();
        let plugin_configuration: BTreeMap<String, String> =
            BTreeMap::try_from(&protobuf_configuration).unwrap();
        state.borrow_mut().load(plugin_configuration);
    });
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn update() -> bool {
    STATE.with(|state| {
        let protobuf_bytes: Vec<u8> = zellij_tile::shim::object_from_stdin().unwrap();
        let protobuf_event = ProtobufEvent::decode(protobuf_bytes.as_slice()).unwrap();
        let event = protobuf_event.try_into().unwrap();
        state.borrow_mut().update(event)
    })
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn pipe() -> bool {
    STATE.with(|state| {
        let protobuf_bytes: Vec<u8> = zellij_tile::shim::object_from_stdin().unwrap();
        let protobuf_pipe_message = ProtobufPipeMessage::decode(protobuf_bytes.as_slice()).unwrap();
        let pipe_message = protobuf_pipe_message.try_into().unwrap();
        state.borrow_mut().pipe(pipe_message)
    })
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn render(rows: i32, cols: i32) {
    STATE.with(|state| {
        state.borrow_mut().render(rows as usize, cols as usize);
    });
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn plugin_version() {
    println!("{}", VERSION);
}

#[cfg(test)]
#[unsafe(no_mangle)]
extern "C" fn host_run_plugin_command() {}
