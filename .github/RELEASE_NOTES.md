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
tar -xzf mdview-macos-universal-app.tar.gz
mv mdview.app /Applications/
```

The build is signed ad-hoc rather than with an Apple Developer ID, so macOS will
not vouch for it. If it refuses to open, clear the quarantine flag the browser
attached to the download:

```sh
xattr -dr com.apple.quarantine /Applications/mdview.app
```

Then open it once from Finder. To make it your default Markdown viewer,
right-click any `.md` file → *Get Info* → *Open with* → mdview → *Change All…*.

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

## A note on signing

These binaries are **not** signed with an Apple Developer ID or an Authenticode
certificate — both require a paid subscription. That is why macOS and Windows
warn about them, and the warnings are the operating system doing its job: it has
no way to confirm who produced the file.

If you would rather not take that on trust, building from source takes about a
minute and produces exactly what is here:

```sh
git clone https://github.com/yowmamasita/mdview
cd mdview
cargo install --path .          # command line
scripts/bundle-macos.sh --install   # macOS application, if you want it
```
