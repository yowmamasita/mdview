A lightweight desktop viewer for Markdown. It opens a file, renders it, and does
nothing else.

CommonMark 0.31.2 (652/652 specification examples), GitHub Flavored Markdown,
Mermaid diagrams rendered offline, live reload on save.

## Download

| Platform | File |
| --- | --- |
| macOS (Apple Silicon + Intel) | `mdview-macos-universal-app.tar.gz` — the application |
| macOS, command line only | `mdview-macos-universal.tar.gz` |
| Linux x86_64 | `mdview-linux-x86_64.tar.gz` |
| Linux aarch64 | `mdview-linux-aarch64.tar.gz` |
| Windows x86_64 | `mdview-windows-x86_64.zip` |
| Windows aarch64 | `mdview-windows-aarch64.zip` |

Verify a download against `SHA256SUMS`:

```sh
sha256sum --check --ignore-missing SHA256SUMS
```

## Install

### macOS

```sh
curl -fsSL https://raw.githubusercontent.com/yowmamasita/mdview/main/scripts/install-macos.sh | sh
```

This is the path of least resistance, and not only for convenience: macOS
attaches a quarantine flag to anything a *browser* downloads, and an application
signed ad-hoc rather than with a paid Developer ID cannot get past Gatekeeper
once it carries that flag. A file fetched by curl is never quarantined, so the
application simply opens. The script checks the published SHA-256 before
installing.

If you would rather download the tarball by hand, you have to clear the flag
yourself:

```sh
tar -xzf mdview-macos-universal-app.tar.gz
mv mdview.app /Applications/
xattr -dr com.apple.quarantine /Applications/mdview.app
```

To make it your default Markdown viewer, right-click any `.md` file →
*Get Info* → *Open with* → mdview → *Change All…*.

### Linux

```sh
tar -xzf mdview-linux-x86_64.tar.gz
cd mdview-linux-x86_64
./install.sh
```

Installs to `~/.local`, registers the desktop entry and sets mdview as the
default Markdown handler. No root needed. Requires GTK 3 and WebKitGTK 4.1:

```sh
sudo apt install libgtk-3-0 libwebkit2gtk-4.1-0     # Debian / Ubuntu
sudo dnf install gtk3 webkit2gtk4.1                 # Fedora
```

Built on Ubuntu 22.04, so it needs glibc 2.35 or newer.

### Windows

Unzip and run `mdview.exe`. SmartScreen will warn about an unrecognised
publisher — *More info* → *Run anyway*. Needs the WebView2 runtime, which ships
with Windows 11 and current Windows 10.

To make it the default: right-click a `.md` file → *Open with* → *Choose another
app* → browse to `mdview.exe` → *Always use this app*.

The zip contains two executables. `mdview.exe` is the viewer, built without a
console so opening a document raises no black window behind it. Neither `cmd`
nor PowerShell waits for a program built that way, which makes redirecting its
output unreliable, so `mdview-console.exe` is the same program with a console
attached — use it in scripts:

```
mdview-console.exe --print-html doc.md > doc.html
```

## A note on signing

These binaries are **not** signed with an Apple Developer ID or an Authenticode
certificate — both require a paid subscription. That is why macOS and Windows
warn about them, and the warnings are the operating system doing its job: it has
no way to confirm who produced the file.

The macOS installer above avoids the warning rather than suppressing it. It does
not make the build any more trustworthy than the tarball; it just means macOS
never marks it as having come from the web.

If you would rather not take that on trust, building from source takes about a
minute and produces exactly what is here:

```sh
git clone https://github.com/yowmamasita/mdview
cd mdview
cargo install --path .          # command line
scripts/bundle-macos.sh --install   # macOS application, if you want it
```
