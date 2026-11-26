// Scratch UI automation. Never activate an app or post to the global event stream.
import AppKit
import ApplicationServices
import Foundation

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data(("mac UI: " + message + "\n").utf8))
    exit(1)
}

let args = CommandLine.arguments
guard args.count == 6, let pid = Int32(args[1]), pid > 0,
      let windowID = UInt32(args[2]), windowID > 0 else {
    fail("expected a scratch PID and window ID")
}
let expectedExe = URL(fileURLWithPath: args[3]).resolvingSymlinksInPath().path
let expectedTitle = args[4]
guard let app = NSRunningApplication(processIdentifier: pid),
      app.executableURL?.resolvingSymlinksInPath().path == expectedExe,
      let windows = CGWindowListCopyWindowInfo(.optionIncludingWindow, windowID) as? [[String: Any]],
      windows.contains(where: {
          $0[kCGWindowNumber as String] as? UInt32 == windowID &&
          $0[kCGWindowOwnerPID as String] as? Int32 == pid &&
          $0[kCGWindowLayer as String] as? Int == 0
      }) else {
    fail("the scratch process does not own the expected window")
}

func attribute(_ element: AXUIElement, _ name: String) -> CFTypeRef? {
    var value: CFTypeRef?
    guard AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success else {
        return nil
    }
    return value
}

func title(_ element: AXUIElement) -> String? { attribute(element, kAXTitleAttribute) as? String }
func children(_ element: AXUIElement) -> [AXUIElement] {
    attribute(element, kAXChildrenAttribute) as? [AXUIElement] ?? []
}
func press(_ element: AXUIElement) {
    guard AXUIElementPerformAction(element, kAXPressAction as CFString) == .success else {
        fail("scratch menu action failed")
    }
}

let ax = AXUIElementCreateApplication(pid)
// These harnesses expect exactly one window. Do not act on another window or document.
guard let appWindows = attribute(ax, kAXWindowsAttribute) as? [AXUIElement],
      appWindows.count == 1, title(appWindows[0]) == expectedTitle else {
    fail("the scratch window does not show the expected document")
}

switch args[5] {
case "verify":
    break
case "edit":
    // Edit in mdedit is an in-window shortcut, not a native menu item. Send both
    // events directly to the checked PID without making its window frontmost.
    guard let down = CGEvent(keyboardEventSource: nil, virtualKey: 14, keyDown: true),
          let up = CGEvent(keyboardEventSource: nil, virtualKey: 14, keyDown: false) else {
        fail("could not create the Edit in mdedit shortcut")
    }
    down.flags = .maskCommand
    up.flags = .maskCommand
    down.postToPid(pid)
    up.postToPid(pid)
case "menu", "quit":
    guard let value = attribute(ax, kAXMenuBarAttribute),
          CFGetTypeID(value) == AXUIElementGetTypeID() else {
        fail("scratch app has no accessible menu bar")
    }
    let bar = value as! AXUIElement
    guard let item = children(bar).first(where: { title($0) == "mdview" }) else {
        fail("scratch app has no application menu")
    }
    if args[5] == "menu" {
        press(item)
    } else {
        guard let quit = children(item).flatMap(children).first(where: { title($0) == "Quit mdview" }) else {
            fail("scratch app menu has no Quit item")
        }
        press(quit)
    }
default:
    fail("unknown scratch UI action")
}
