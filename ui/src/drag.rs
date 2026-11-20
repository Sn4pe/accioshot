// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::PathBuf;

use serde_json::Value;
use tauri::ipc::{Invoke, InvokeBody};
use tauri::plugin::{Plugin, TauriPlugin};
use tauri::{AppHandle, Manager, Runtime};

use crate::history::Library;

pub struct Drag<R: Runtime>(TauriPlugin<R>);

pub fn init<R: Runtime>() -> Drag<R> {
    Drag(tauri_plugin_drag::init())
}

impl<R: Runtime> Plugin<R> for Drag<R> {
    fn name(&self) -> &'static str {
        self.0.name()
    }

    fn initialize(
        &mut self,
        app: &AppHandle<R>,
        config: Value,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.0.initialize(app, config)
    }

    fn initialization_script(&self) -> Option<String> {
        self.0.initialization_script()
    }

    fn extend_api(&mut self, invoke: Invoke<R>) -> bool {
        let allowed = offered_files(invoke.message.payload()).is_some_and(|files| {
            invoke
                .message
                .webview()
                .try_state::<Library>()
                .is_some_and(|library| library.holds_files(&files))
        });
        if !allowed {
            invoke
                .resolver
                .reject("only files in the history can be dragged");
            return true;
        }
        self.0.extend_api(invoke)
    }
}

fn offered_files(payload: &InvokeBody) -> Option<Vec<PathBuf>> {
    let InvokeBody::Json(body) = payload else {
        return None;
    };
    serde_json::from_value(body.get("item")?.clone()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_a_list_of_files_is_offered() {
        let files = json!({ "item": ["/a.png", "/b.png"], "image": "x" });
        assert_eq!(
            offered_files(&InvokeBody::Json(files)),
            Some(vec![PathBuf::from("/a.png"), PathBuf::from("/b.png")])
        );
        let data = json!({ "item": { "data": "x", "types": ["text/plain"] } });
        assert_eq!(offered_files(&InvokeBody::Json(data)), None);
        assert_eq!(offered_files(&InvokeBody::Json(json!({}))), None);
        assert_eq!(offered_files(&InvokeBody::Raw(Vec::new())), None);
    }
}
