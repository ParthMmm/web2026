use anyhow::{Context, Result, bail};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, SyncSender},
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_SESSION_ATTEMPTS: u32 = 100;
const JPEG_TYPE_IDENTIFIER: &str = "public.jpeg";
const IMAGE_TYPE_IDENTIFIER: &str = "public.image";
const COMPATIBLE_REPRESENTATION_MODE: isize = 2;
const STATE_IVAR: &str = "photoImportState";

/// Return the app-managed root used for copied Photos assets. Imported copies
/// become the photos' originals, so they live beside the library catalog.
pub fn import_root(library_root: &Path) -> PathBuf {
    std::env::var_os("PHOTO_IMPORT_DIR").map_or_else(|| library_root.join("imports"), PathBuf::from)
}

/// Create a unique, empty directory owned by one Photos import.
pub fn create_import_session(root: &Path) -> Result<PathBuf> {
    fs::create_dir_all(root)
        .with_context(|| format!("create Photos import directory {}", root.display()))?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before the Unix epoch")?
        .as_nanos();
    let process_id = std::process::id();
    for attempt in 0..MAX_SESSION_ATTEMPTS {
        let session = root.join(format!("session-{process_id}-{timestamp}-{attempt}"));
        match fs::create_dir(&session) {
            Ok(()) => return Ok(session),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!("create Photos import session {}", session.display())
                });
            }
        }
    }
    bail!(
        "could not create a unique Photos import session in {}",
        root.display()
    )
}

/// Remove files copied by a Photos import that failed in the local pipeline.
pub fn cleanup_import(paths: &[PathBuf]) {
    for path in paths {
        let _ = fs::remove_file(path);
    }
    if let Some(session) = paths.first().and_then(|path| path.parent())
        && paths.iter().all(|path| path.parent() == Some(session))
        && session
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("session-"))
    {
        let _ = fs::remove_dir(session);
    }
}

#[cfg(target_os = "macos")]
#[allow(deprecated, unexpected_cfgs)]
mod macos {
    use super::*;
    use block::ConcreteBlock;
    use cocoa::{
        appkit::{NSApplication, NSBackingStoreBuffered, NSWindow, NSWindowStyleMask},
        base::{NO, YES, id, nil},
        foundation::{NSArray, NSPoint, NSRect, NSSize, NSString, NSURL},
    };
    use objc::{
        class,
        declare::ClassDecl,
        msg_send,
        runtime::{Class, Object, Sel},
        sel, sel_impl,
    };
    use std::{
        ffi::{CStr, c_char, c_void},
        path::Path,
        sync::{Arc, Mutex, OnceLock},
    };

    #[link(name = "PhotosUI", kind = "framework")]
    unsafe extern "C" {}

    struct ImportState {
        destination: PathBuf,
        pending: usize,
        picker_finished: bool,
        outputs: Vec<Option<PathBuf>>,
        failure: Option<String>,
        sender: Option<SyncSender<Result<Vec<PathBuf>, String>>>,
    }

    /// Present the native Photos picker and return its copied JPEG paths.
    pub fn open(root: PathBuf) -> Result<Receiver<Result<Vec<PathBuf>, String>>> {
        let destination = create_import_session(&root)?;
        let (sender, receiver) = mpsc::sync_channel(1);
        let state = Arc::new(Mutex::new(ImportState {
            destination,
            pending: 0,
            picker_finished: false,
            outputs: Vec::new(),
            failure: None,
            sender: Some(sender),
        }));

        if let Err(error) = show_picker(Arc::clone(&state)) {
            cleanup_state(&state);
            return Err(error);
        }
        Ok(receiver)
    }

    fn show_picker(state: Arc<Mutex<ImportState>>) -> Result<()> {
        objc::rc::autoreleasepool(|| unsafe {
            for class_name in [
                "PHPickerConfiguration",
                "PHPickerFilter",
                "PHPickerViewController",
            ] {
                if Class::get(class_name).is_none() {
                    bail!("macOS 13 or later is required for Photos import")
                }
            }

            let on_main_thread: bool = msg_send![class!(NSThread), isMainThread];
            if !on_main_thread {
                bail!("Photos picker must open on the main thread")
            }
            let app = NSApplication::sharedApplication(nil);
            let key_window: id = msg_send![app, keyWindow];
            let parent: id = if key_window == nil {
                msg_send![app, mainWindow]
            } else {
                key_window
            };
            if parent == nil {
                bail!("the Photo prototype window is not available")
            }

            let configuration: id = msg_send![class!(PHPickerConfiguration), new];
            if configuration == nil {
                bail!("could not create the Photos picker configuration")
            }
            let filter: id = msg_send![class!(PHPickerFilter), imagesFilter];
            if filter == nil {
                let _: () = msg_send![configuration, release];
                bail!("could not create the Photos image filter")
            }
            let _: () = msg_send![configuration, setFilter: filter];
            let _: () = msg_send![configuration, setSelectionLimit: 0isize];
            // Ask Photos for a representation suitable for this JPEG pipeline. If
            // it only offers another image type, copy_selected_file converts it.
            let _: () = msg_send![
                configuration,
                setPreferredAssetRepresentationMode: COMPATIBLE_REPRESENTATION_MODE
            ];

            let picker: id = msg_send![class!(PHPickerViewController), alloc];
            let picker: id = msg_send![picker, initWithConfiguration: configuration];
            let _: () = msg_send![configuration, release];
            if picker == nil {
                bail!("could not create the Photos picker")
            }

            let delegate_class = picker_delegate_class();
            let delegate: id = msg_send![delegate_class, new];
            if delegate == nil {
                let _: () = msg_send![picker, release];
                bail!("could not create the Photos picker delegate")
            }
            let state_pointer = Box::into_raw(Box::new(Arc::clone(&state))) as *mut c_void;
            (*delegate).set_ivar(STATE_IVAR, state_pointer);
            let _: () = msg_send![picker, setDelegate: delegate];

            let sheet = NSWindow::alloc(nil).initWithContentRect_styleMask_backing_defer_(
                NSRect::new(NSPoint::new(0., 0.), NSSize::new(760., 640.)),
                NSWindowStyleMask::NSTitledWindowMask,
                NSBackingStoreBuffered,
                NO,
            );
            if sheet == nil {
                let _: () = msg_send![picker, release];
                let _: () = msg_send![delegate, release];
                bail!("could not create the Photos picker window")
            }
            let title = NSString::alloc(nil).init_str("Import from Photos");
            sheet.setTitle_(title);
            let _: () = msg_send![title, release];
            sheet.setReleasedWhenClosed_(YES);
            let _: () = msg_send![sheet, setContentViewController: picker];
            // PHPickerViewController inherits NSViewController on macOS, whose
            // representedObject retains the weak picker delegate until dismissal.
            let _: () = msg_send![picker, setRepresentedObject: delegate];

            let _: () = msg_send![parent, beginSheet: sheet completionHandler: nil];
            let _: () = msg_send![picker, release];
            let _: () = msg_send![delegate, release];
            let _: () = msg_send![sheet, release];
            Ok(())
        })
    }

    fn picker_delegate_class() -> &'static Class {
        static CLASS: OnceLock<usize> = OnceLock::new();
        let pointer = CLASS.get_or_init(|| unsafe {
            let mut declaration = ClassDecl::new("PhotoPrototypePickerDelegate", class!(NSObject))
                .expect("Photos picker delegate class name is unused");
            declaration.add_ivar::<*mut c_void>(STATE_IVAR);
            declaration.add_method(
                sel!(dealloc),
                picker_delegate_dealloc as extern "C" fn(&Object, Sel),
            );
            declaration.add_method(
                sel!(picker:didFinishPicking:),
                picker_did_finish_picking as extern "C" fn(&Object, Sel, id, id),
            );
            declaration.register() as *const Class as usize
        });
        unsafe { &*(*pointer as *const Class) }
    }

    fn abandon_import_if_picker_not_finished(state: &Arc<Mutex<ImportState>>) {
        let abandoned = {
            let mut import = state.lock().expect("Photos import state is not poisoned");
            if import.picker_finished || import.sender.is_none() {
                false
            } else {
                import.sender.take();
                true
            }
        };
        if abandoned {
            cleanup_state(state);
        }
    }

    extern "C" fn picker_delegate_dealloc(this: &Object, _: Sel) {
        unsafe {
            let state = *this.get_ivar::<*mut c_void>(STATE_IVAR);
            if !state.is_null() {
                let state = Box::from_raw(state as *mut Arc<Mutex<ImportState>>);
                abandon_import_if_picker_not_finished(&state);
                drop(state);
            }
            let _: () = msg_send![super(this, class!(NSObject)), dealloc];
        }
    }

    extern "C" fn picker_did_finish_picking(this: &Object, _: Sel, picker: id, results: id) {
        unsafe {
            let state_pointer = *this.get_ivar::<*mut c_void>(STATE_IVAR);
            if state_pointer.is_null() {
                return;
            }
            let state = Arc::clone(&*(state_pointer as *const Arc<Mutex<ImportState>>));
            state
                .lock()
                .expect("Photos import state is not poisoned")
                .picker_finished = true;
            dismiss_picker(picker);

            let count = match usize::try_from(NSArray::count(results)) {
                Ok(count) => count,
                Err(_) => {
                    complete_import(
                        &state,
                        Err("Photos returned too many selected assets".into()),
                    );
                    return;
                }
            };
            if count == 0 {
                complete_import(&state, Ok(Vec::new()));
                return;
            }

            {
                let mut import = state.lock().expect("Photos import state is not poisoned");
                import.pending = count;
                import.outputs = vec![None; count];
            }

            for index in 0..count {
                let result: id = NSArray::objectAtIndex(
                    results,
                    u64::try_from(index).expect("selected asset index fits NSUInteger"),
                );
                let provider: id = msg_send![result, itemProvider];
                let Some(type_identifier) = provider_type_identifier(provider) else {
                    record_file_result(
                        &state,
                        index,
                        Err("the selected Photos asset has no image representation".into()),
                    );
                    continue;
                };
                let state_for_block = Arc::clone(&state);
                let block = ConcreteBlock::new(move |url: id, error: id| {
                    objc::rc::autoreleasepool(|| {
                        let result = if error != nil {
                            Err(objective_c_error(error))
                        } else if url == nil {
                            Err("Photos returned no file for the selected asset".into())
                        } else {
                            match url_to_path(url) {
                                Some(path) => {
                                    let destination = state_for_block
                                        .lock()
                                        .expect("Photos import state is not poisoned")
                                        .destination
                                        .clone();
                                    copy_selected_file(&path, &destination, index, type_identifier)
                                }
                                None => Err("Photos returned a file URL without a path".into()),
                            }
                        };
                        record_file_result(&state_for_block, index, result);
                    });
                })
                .copy();
                let type_string = NSString::alloc(nil).init_str(type_identifier);
                let progress: id = msg_send![
                    provider,
                    loadFileRepresentationForTypeIdentifier: type_string
                    completionHandler: block
                ];
                let _: () = msg_send![type_string, release];
                if progress == nil {
                    record_file_result(
                        &state,
                        index,
                        Err("Photos could not start loading the selected asset".into()),
                    );
                }
            }
        }
    }

    unsafe fn dismiss_picker(picker: id) {
        let view: id = msg_send![picker, view];
        let sheet: id = msg_send![view, window];
        if sheet == nil {
            return;
        }
        let parent: id = msg_send![sheet, sheetParent];
        if parent != nil {
            let _: () = msg_send![parent, endSheet: sheet];
        } else {
            let _: () = msg_send![sheet, close];
        }
    }

    unsafe fn provider_type_identifier(provider: id) -> Option<&'static str> {
        let jpeg = unsafe { NSString::alloc(nil).init_str(JPEG_TYPE_IDENTIFIER) };
        let has_jpeg: bool = msg_send![provider, hasItemConformingToTypeIdentifier: jpeg];
        let _: () = msg_send![jpeg, release];
        if has_jpeg {
            return Some(JPEG_TYPE_IDENTIFIER);
        }

        let image = unsafe { NSString::alloc(nil).init_str(IMAGE_TYPE_IDENTIFIER) };
        let has_image: bool = msg_send![provider, hasItemConformingToTypeIdentifier: image];
        let _: () = msg_send![image, release];
        has_image.then_some(IMAGE_TYPE_IDENTIFIER)
    }

    fn url_to_path(url: id) -> Option<PathBuf> {
        unsafe {
            let path: id = NSURL::path(url);
            string_from_ns_object(path).map(PathBuf::from)
        }
    }

    fn objective_c_error(error: id) -> String {
        unsafe {
            let description: id = msg_send![error, localizedDescription];
            string_from_ns_object(description)
                .unwrap_or_else(|| "Photos could not load the selected asset".into())
        }
    }

    fn string_from_ns_object(value: id) -> Option<String> {
        if value == nil {
            return None;
        }
        unsafe {
            let pointer: *const c_char = msg_send![value, UTF8String];
            if pointer.is_null() {
                return None;
            }
            CStr::from_ptr(pointer).to_str().ok().map(str::to_owned)
        }
    }

    fn copy_selected_file(
        source: &Path,
        destination: &Path,
        index: usize,
        type_identifier: &str,
    ) -> Result<PathBuf, String> {
        let output = destination.join(format!("photo-{index:06}.jpg"));
        let temporary = destination.join(format!("photo-{index:06}.tmp.jpg"));
        let result = if type_identifier == JPEG_TYPE_IDENTIFIER {
            fs::copy(source, &temporary)
                .map(|_| ())
                .map_err(|error| format!("copy selected Photos asset: {error}"))
        } else {
            let status = std::process::Command::new("/usr/bin/sips")
                .args(["-s", "format", "jpeg"])
                .arg(source)
                .args(["--out"])
                .arg(&temporary)
                .output()
                .map_err(|error| format!("convert selected Photos asset to JPEG: {error}"))?;
            if status.status.success() {
                Ok(())
            } else {
                Err(format!(
                    "convert selected Photos asset to JPEG: {}",
                    String::from_utf8_lossy(&status.stderr).trim()
                ))
            }
        };
        if let Err(error) = result {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
        if let Err(error) = fs::rename(&temporary, &output) {
            let _ = fs::remove_file(&temporary);
            return Err(format!("store selected Photos asset: {error}"));
        }
        Ok(output)
    }

    fn record_file_result(
        state: &Arc<Mutex<ImportState>>,
        index: usize,
        result: Result<PathBuf, String>,
    ) {
        let completion = {
            let mut import = state.lock().expect("Photos import state is not poisoned");
            if import.sender.is_none() {
                return;
            }
            match result {
                Ok(path) => import.outputs[index] = Some(path),
                Err(error) => {
                    import.failure.get_or_insert(error);
                }
            }
            import.pending = import.pending.saturating_sub(1);
            if import.pending != 0 {
                return;
            }
            let result = if let Some(error) = import.failure.take() {
                Err(error)
            } else {
                Ok(import
                    .outputs
                    .iter_mut()
                    .map(Option::take)
                    .collect::<Option<Vec<_>>>()
                    .unwrap_or_default())
            };
            import.sender.take().map(|sender| (sender, result))
        };
        if let Some((sender, result)) = completion {
            complete_import_with_sender(state, sender, result);
        }
    }

    fn complete_import(state: &Arc<Mutex<ImportState>>, result: Result<Vec<PathBuf>, String>) {
        let sender = state
            .lock()
            .expect("Photos import state is not poisoned")
            .sender
            .take();
        if let Some(sender) = sender {
            complete_import_with_sender(state, sender, result);
        }
    }

    fn complete_import_with_sender(
        state: &Arc<Mutex<ImportState>>,
        sender: SyncSender<Result<Vec<PathBuf>, String>>,
        result: Result<Vec<PathBuf>, String>,
    ) {
        if result.as_ref().is_err() || result.as_ref().is_ok_and(Vec::is_empty) {
            cleanup_state(state);
        }
        if sender.send(result).is_err() {
            cleanup_state(state);
        }
    }

    fn cleanup_state(state: &Arc<Mutex<ImportState>>) {
        let destination = state
            .lock()
            .expect("Photos import state is not poisoned")
            .destination
            .clone();
        let _ = fs::remove_dir_all(destination);
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn jpeg_representation_is_copied_even_without_a_file_extension() {
            let temporary = tempfile::tempdir().unwrap();
            let source = temporary.path().join("provider-file");
            let destination = temporary.path().join("session");
            fs::create_dir(&destination).unwrap();
            fs::write(&source, b"jpeg representation").unwrap();

            let output =
                copy_selected_file(&source, &destination, 3, JPEG_TYPE_IDENTIFIER).unwrap();

            assert_eq!(fs::read(&output).unwrap(), b"jpeg representation");
            assert!(!destination.join("photo-000003.tmp.jpg").exists());
        }

        #[test]
        fn failed_receiver_delivery_removes_a_completed_session() {
            let temporary = tempfile::tempdir().unwrap();
            let destination = temporary.path().join("session");
            fs::create_dir(&destination).unwrap();
            let output = destination.join("photo-000000.jpg");
            fs::write(&output, b"jpeg representation").unwrap();
            let (sender, receiver) = mpsc::sync_channel(1);
            let state = Arc::new(Mutex::new(ImportState {
                destination: destination.clone(),
                pending: 0,
                picker_finished: true,
                outputs: vec![Some(output.clone())],
                failure: None,
                sender: Some(sender),
            }));
            let sender = state
                .lock()
                .unwrap()
                .sender
                .take()
                .expect("test state has a sender");
            drop(receiver);

            complete_import_with_sender(&state, sender, Ok(vec![output]));

            assert!(!destination.exists());
        }

        #[test]
        fn destroying_an_unfinished_picker_disconnects_and_cleans_up() {
            let temporary = tempfile::tempdir().unwrap();
            let destination = temporary.path().join("session");
            fs::create_dir(&destination).unwrap();
            let (sender, receiver) = mpsc::sync_channel(1);
            let state = Arc::new(Mutex::new(ImportState {
                destination: destination.clone(),
                pending: 0,
                picker_finished: false,
                outputs: Vec::new(),
                failure: None,
                sender: Some(sender),
            }));

            abandon_import_if_picker_not_finished(&state);

            assert!(receiver.try_recv().is_err());
            assert!(!destination.exists());
        }
    }
}

#[cfg(target_os = "macos")]
pub use macos::open;

#[cfg(not(target_os = "macos"))]
pub fn open(_destination: PathBuf) -> Result<Receiver<Result<Vec<PathBuf>, String>>> {
    bail!("Photos import is available only on macOS 13 or later")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_sessions_are_unique_and_nested_under_the_app_root() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("imports");
        let first = create_import_session(&root).unwrap();
        let second = create_import_session(&root).unwrap();

        assert_ne!(first, second);
        assert_eq!(first.parent(), Some(root.as_path()));
        assert_eq!(second.parent(), Some(root.as_path()));
        assert!(first.is_dir());
        assert!(second.is_dir());
    }

    #[test]
    fn cleanup_import_removes_only_the_owned_session_files() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("imports");
        let session = create_import_session(&root).unwrap();
        let first = session.join("photo-000000.jpg");
        let second = session.join("photo-000001.jpg");
        let unrelated = session.join("keep.txt");
        fs::write(&first, b"first").unwrap();
        fs::write(&second, b"second").unwrap();
        fs::write(&unrelated, b"keep").unwrap();

        cleanup_import(&[first.clone(), second.clone()]);

        assert!(!first.exists());
        assert!(!second.exists());
        assert!(unrelated.exists());
        assert!(session.exists());
    }
}
