<div align="center">
  <h1>B⧸REACH</h1>
  <p>
    <strong>Speed. Precision. Unambiguity.</strong><br>
    The single-file web development ecosystem.
  </p>

  <a href="https://github.com/Notwinner0/b-reach/actions"><img src="https://github.com/Notwinner0/b-reach/actions/workflows/rust.yml/badge.svg?branch=main" alt="Build Status"></a>
  <a href="http://github.com/Notwinner0/b-reach/releases"><img src="https://img.shields.io/github/v/tag/Notwinner0/b-reach" alt="Version"></a>
  <a href="https://github.com/Notwinner0/b-reach?tab=MIT-1-ov-file#readme"><img src="https://img.shields.io/github/license/Notwinner0/b-reach" alt="License"></a>
  <a href="https://github.com/Notwinner0/b-reach/graphs/contributors"><img src="https://img.shields.io/github/contributors/Notwinner0/b-reach" alt="Contributors"></a>
  <a href="https://github.com/Notwinner0/b-reach/stargazers"><img src="https://img.shields.io/github/stars/Notwinner0/b-reach?style=flat" alt="Stars"></a>
</div>

---

**B⧸REACH** is a rapid prototyping ecosystem designed to eliminate context switching. It unifies development, deployment, and hosting into a single, cohesive workflow, powered by a high-performance Rust core.

> **⚠️ Project Status: Experimental**
> This project is currently in early-stage development (v0.0.0). Features are evolving rapidly. Use for prototyping and development.
> This is not even a minimum viable product (MVP). There are no tests, and there's no guarantee that something won't go wrong. Use at your own risk, especially in production environments.

## 🌟 The Vision

B⧸REACH is built on three pillars:

1.  **The Server:** A blazing fast, single-executable web server for rapid prototyping. It treats a single plain-text file as a full-stack application using language delimiters.
2.  **The Bridge:** An automated CLI tool to deploy your prototypes instantly to major cloud platforms (Netlify, Vercel, GitHub Pages).
3.  **The Cloud:** An independent, simplified hosting service (no AWS/Azure/GCP complexity) for when you are ready to go live (Paid Service).

## ✨ Core Features

* **Language Agnostic:** Write in your preferred syntax. B⧸REACH parses delimiters to handle compilation automatically.
    * *Markup:* HTML, Markdown, XML, YAML, TOML, Pug, HAML...
    * *Script:* JavaScript, TypeScript, Gleam, Haxe, WASM, CoffeeScript...
    * *Style:* CSS, SCSS (SASS), Less...
* **Single-File Architecture:** Keep your structure, logic, and styling in one `.breach` file. No complex folder structures for simple prototypes.
* **Rust Powered:** Built on `tokio`, `ntex`, and `grass` for safety and speed.
* **Live Reload by Default:** Instant feedback via WebSocket injection. Save the file, see the change.
* **TUI & CLI:** Fully controllable via terminal user interface (GUI is not planned).

## 📦 Installation

### Pre-built Binaries
*(Coming soon via GitHub Releases)*

### Building from Source

Requirements: Rust toolchain (cargo).

```sh
git clone [https://github.com/Notwinner/b-reach.git](https://github.com/Notwinner/b-reach.git)
cd b-reach
cargo build --release
```

The binary will be located at `./target/release/b-reach`. Ensure this is in your system `$PATH`.

## 🚀 Usage

### 1\. The `.breach` File Format

B⧸REACH files use specific delimiters (`¦`) followed by the language tag to separate code sections. You can mix and match languages.

**Create a file named `app.breach`:**

```text
¦html
<!DOCTYPE html>
<html lang="en">
<head>
    <title>My B⧸REACH App</title>
</head>
<body>
    <div class="hero">
        <h1>Prototype Fast.</h1>
        <p id="dynamic-text">Loading...</p>
    </div>
</body>
</html>

¦scss
$primary: #ff5722;
$dark: #212121;

body {
    background: $dark;
    color: white;
    font-family: system-ui, sans-serif;
    display: grid;
    place-items: center;
    height: 100vh;
    margin: 0;
}

.hero {
    text-align: center;
    h1 { color: $primary; }
}

¦ts
// B⧸REACH handles the compilation
const updateMessage = (msg: string) => {
    const el = document.getElementById('dynamic-text');
    if (el) el.innerText = msg;
};

setTimeout(() => {
    updateMessage("Deployed with precision.");
}, 1000);
```

### 2\. Running the Server

Simply run the command in the directory containing your file:

```sh
b-reach
```

  * **Localhost:** Opens `http://127.0.0.1:8080`
  * **Live Reload:** Active at `/ws`
  * **Assets:** CSS injected at `/style.css`, JS at `/script.js`

## 🗺️ Roadmap
In no particular order:
  - [x] Basic HTML/CSS/JS parsing
  - [x] SCSS Compilation (via `grass`)
  - [x] Live Reload (WebSocket)
  - [ ] Comprehensive unit and intergration tests
  - [ ] First MVP release
  - [ ] **TUI Implementation:** Interactive terminal dashboard
  - [ ] **Polyglot Support:** Add compilers for TypeScript, Pug, and Markdown
  - [ ] **The Bridge:** API integration for Vercel/Netlify deployment
  - [ ] **B⧸REACH Cloud:** Native hosting integration

## 🤝 Contributing

B⧸REACH is open source and we welcome contributions\!

1.  Fork the repository.
2.  Create your feature branch (`git checkout -b feature/amazing-feature`).
3.  Commit your changes (`git commit -m 'Add amazing feature'`).
4.  Push to the branch (`git push origin feature/amazing-feature`).
5.  Open a Pull Request.

License
-------

The MIT License (MIT)

Copyright (c) 2025 Notwinner

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
