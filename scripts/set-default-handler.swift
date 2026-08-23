#!/usr/bin/env swift
//
// Point Launch Services at a bundle identifier for Markdown documents.
//
//     scripts/set-default-handler.swift io.github.yowmamasita.mdview
//
// This is what Finder's "Get Info → Open with → Change All" does, and what
// `duti` does; doing it here avoids depending on either.

import AppKit
import CoreServices
import Foundation
import UniformTypeIdentifiers

/// Extensions a Markdown file might carry. `md` and `markdown` resolve to the
/// real type; the rest have no declaration on most systems and resolve to a
/// dynamic placeholder, which still has to be claimed explicitly.
let extensions = [
    "md", "markdown", "mdown", "mkd", "mkdn", "mdwn", "mdtxt", "mdtext", "rmd", "qmd",
]

guard CommandLine.arguments.count == 2 else {
    FileHandle.standardError.write("usage: set-default-handler.swift <bundle-id>\n".data(using: .utf8)!)
    exit(2)
}
let bundleID = CommandLine.arguments[1]

/// Every type identifier the extensions above actually resolve to.
func markdownTypeIdentifiers() -> [String] {
    var identifiers = Set(["net.daringfireball.markdown"])
    for ext in extensions {
        if let type = UTType(filenameExtension: ext) {
            identifiers.insert(type.identifier)
        }
    }
    return identifiers.sorted()
}

var claimed = 0
var refused: [String] = []

for identifier in markdownTypeIdentifiers() {
    let status = LSSetDefaultRoleHandlerForContentType(
        identifier as CFString, .all, bundleID as CFString
    )
    if status == noErr {
        claimed += 1
        print("  \(identifier)")
    } else {
        refused.append("\(identifier) (OSStatus \(status))")
    }
}

print("  claimed \(claimed) type(s) for \(bundleID)")

// A refusal is worth mentioning but is not a failure: Launch Services declines
// some placeholder types outright, and those files can still be pointed at the
// application through Finder's "Change All".
if !refused.isEmpty {
    FileHandle.standardError.write(
        "  not claimable: \(refused.joined(separator: ", "))\n".data(using: .utf8)!
    )
}
