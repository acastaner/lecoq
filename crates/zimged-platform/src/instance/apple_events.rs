//! macOS file-open delivery (§8.5). Finder's "Open With", a double-click or `open -a` does not put
//! the path in `argv`; it launches (or wakes) the app and sends a `kAEOpenDocuments` Apple Event.
//! winit owns the `NSApplication` delegate and implements no `application:openURLs:`, so AppKit's
//! default handler would reject the document ("cannot open files in the … format"). We instead
//! register our own handler on `NSAppleEventManager`, which is independent of the delegate, and push
//! what it receives into the single-instance [`Inbox`] as an [`Arrival`]. Registering from the
//! eframe creation hook (main thread, after finish-launching) overrides AppKit's default handler.
//!
//! Only the handler glue lives here; the file-list → [`Arrival`] mapping is [`super::open_arrival`],
//! tested on every platform. This module is compiled only on macOS and is verified by building
//! there (the headless gate runs on Linux).

use std::path::PathBuf;
use std::sync::Arc;

use objc2::rc::Retained;
use objc2::runtime::NSObject;
use objc2::{AnyThread, DefinedClass, define_class, msg_send, sel};
use objc2_foundation::{NSAppleEventDescriptor, NSAppleEventManager};

use super::{Arrival, Inbox, open_arrival};

/// A four-character code (`AEEventClass` / `AEEventID` / `AEKeyword` / `DescType`).
const fn fourcc(code: &[u8; 4]) -> u32 {
    ((code[0] as u32) << 24) | ((code[1] as u32) << 16) | ((code[2] as u32) << 8) | (code[3] as u32)
}

const CORE_EVENT_CLASS: u32 = fourcc(b"aevt");
const AE_OPEN_DOCUMENTS: u32 = fourcc(b"odoc");
const AE_REOPEN_APPLICATION: u32 = fourcc(b"rapp");
const KEY_DIRECT_OBJECT: u32 = fourcc(b"----");
const TYPE_FILE_URL: u32 = fourcc(b"furl");

/// What the handler keeps: the inbox arrivals are pushed into.
struct Ivars {
    inbox: Arc<Inbox>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[name = "ZimgedOpenEventHandler"]
    #[ivars = Ivars]
    struct Handler;

    impl Handler {
        /// `NSAppleEventManager` dispatches here on the main thread for both events we register.
        #[unsafe(method(handleAppleEvent:withReplyEvent:))]
        fn handle_apple_event(
            &self,
            event: &NSAppleEventDescriptor,
            _reply: &NSAppleEventDescriptor,
        ) {
            self.dispatch(event);
        }
    }
);

impl Handler {
    fn new(inbox: Arc<Inbox>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(Ivars { inbox });
        unsafe { msg_send![super(this), init] }
    }

    fn dispatch(&self, event: &NSAppleEventDescriptor) {
        let inbox = &self.ivars().inbox;
        // Clicking the Dock icon (reopen) just raises the window; there are no documents.
        if event.eventID() == AE_REOPEN_APPLICATION {
            inbox.push(Arrival::Raise);
            return;
        }
        if let Some(arrival) = open_arrival(file_urls(event)) {
            inbox.push(arrival);
        }
    }
}

/// The file paths carried by an open-documents event's direct object, in order.
fn file_urls(event: &NSAppleEventDescriptor) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let Some(list) = event.paramDescriptorForKeyword(KEY_DIRECT_OBJECT) else {
        return paths;
    };
    // Apple Event lists are 1-based.
    for i in 1..=list.numberOfItems() {
        let Some(item) = list.descriptorAtIndex(i) else {
            continue;
        };
        // Modern Finder sends typeFileURL already; coerce defensively for alias/FSRef senders.
        let coerced = item.coerceToDescriptorType(TYPE_FILE_URL);
        let desc = coerced.as_deref().unwrap_or(&item);
        if let Some(url) = desc.fileURLValue()
            && let Some(path) = url.path()
        {
            paths.push(PathBuf::from(path.to_string()));
        }
    }
    paths
}

/// Register the handler for open-documents and reopen events. Called once, on the main thread.
pub(super) fn install(inbox: Arc<Inbox>) {
    let handler = Handler::new(inbox);
    let manager = NSAppleEventManager::sharedAppleEventManager();
    let selector = sel!(handleAppleEvent:withReplyEvent:);
    // SAFETY: the handler responds to the selector, which takes the two descriptor arguments the
    // manager passes to it.
    unsafe {
        manager.setEventHandler_andSelector_forEventClass_andEventID(
            &handler,
            selector,
            CORE_EVENT_CLASS,
            AE_OPEN_DOCUMENTS,
        );
        manager.setEventHandler_andSelector_forEventClass_andEventID(
            &handler,
            selector,
            CORE_EVENT_CLASS,
            AE_REOPEN_APPLICATION,
        );
    }
    // The manager holds the handler by an unretained reference, and nothing else owns it, so it
    // must live for the rest of the process. Leak it deliberately.
    std::mem::forget(handler);
}
