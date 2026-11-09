//! The Import… sheet: files and folders, several at once. Rune's file dialogs
//! pick one file or one folder, so this drives `NSOpenPanel` directly.

use std::{path::PathBuf, sync::mpsc::Receiver};

/// Presents the panel as a sheet on the key window. The receiver yields the
/// chosen paths, or an empty list when the user cancels.
#[cfg(target_os = "macos")]
#[allow(deprecated, unexpected_cfgs)]
pub(super) fn open() -> anyhow::Result<Receiver<Vec<PathBuf>>> {
    use block::ConcreteBlock;
    use cocoa::{
        appkit::NSApplication,
        base::{YES, id, nil},
        foundation::{NSArray, NSString},
    };
    use objc::{class, msg_send, sel, sel_impl};
    use std::{ffi::CStr, sync::mpsc};

    const RESPONSE_OK: isize = 1;

    let (sender, receiver) = mpsc::sync_channel(1);
    objc::rc::autoreleasepool(|| unsafe {
        let app = NSApplication::sharedApplication(nil);
        let window: id = msg_send![app, keyWindow];
        let window: id = if window == nil {
            msg_send![app, mainWindow]
        } else {
            window
        };
        if window == nil {
            anyhow::bail!("the Photo prototype window is not available")
        }
        let panel: id = msg_send![class!(NSOpenPanel), openPanel];
        let _: () = msg_send![panel, setCanChooseFiles: YES];
        let _: () = msg_send![panel, setCanChooseDirectories: YES];
        let _: () = msg_send![panel, setAllowsMultipleSelection: YES];
        let prompt = NSString::alloc(nil).init_str("Import");
        let _: () = msg_send![panel, setPrompt: prompt];
        let _: () = msg_send![prompt, release];
        let handler = ConcreteBlock::new(move |response: isize| {
            let mut paths = Vec::new();
            if response == RESPONSE_OK {
                let urls: id = msg_send![panel, URLs];
                for index in 0..NSArray::count(urls) {
                    let url = NSArray::objectAtIndex(urls, index);
                    let path: id = msg_send![url, path];
                    let utf8: *const std::ffi::c_char = msg_send![path, UTF8String];
                    if !utf8.is_null() {
                        paths.push(PathBuf::from(
                            CStr::from_ptr(utf8).to_string_lossy().as_ref(),
                        ));
                    }
                }
            }
            let _ = sender.send(paths);
        })
        .copy();
        let _: () = msg_send![panel, beginSheetModalForWindow: window completionHandler: &*handler];
        Ok(())
    })?;
    Ok(receiver)
}

#[cfg(not(target_os = "macos"))]
pub(super) fn open() -> anyhow::Result<Receiver<Vec<PathBuf>>> {
    anyhow::bail!("importing files needs macOS")
}
