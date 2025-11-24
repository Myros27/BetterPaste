# BetterPaste

![BetterPaste Icon](betterPaste.ico)

**BetterPaste** is a bridge between your local codebase and AI Chat interfaces (ChatGPT, Claude, Gemini, DeepSeek). 

It solves the problem of copy pasting diffs from AI by providing a streamlined workflow to generate context-optimized XML and automatically patch your local files based on AI responses.

## 🚀 Features

*   **Context Generator:** Scans your project, respects `.gitignore`, and generates XML context optimized for LLMs.
*   **Smart Patching:** Automatically detects code blocks sent by the AI and applies them to your local files.
*   **Safety First:** Runs entirely on `localhost`. Your code never leaves your machine except when you paste it into the AI.
*   **The Ungenerator:** Can unpack XML context files back into a folder structure (useful for bootstrapping projects).
*   **Diff & Undo:** Review changes before applying them and undo if something breaks.

## 📦 Installation

### 1. The Application
1.  Go to the [Releases](../../releases) page.
2.  Download `betterPaste.exe`.
3.  Run it. (It listens on `http://127.0.0.1:3030`).

### 2. The Browser Script
To allow the AI website to talk to your local BetterPaste app, you need a Userscript.

1.  Install **Tampermonkey** or **Violentmonkey** for your browser.
2.  Create a new script.
3.  Copy the content of [userscript.js](userscript.js) from this repository (or click "Copy Script" inside the BetterPaste Help tab).
4.  Save the script.
5.  **Important:** When the script runs for the first time, your browser will ask permission to connect to `127.0.0.1`. Click **Always Allow**.

## 🛠️ Usage Workflow

1.  **Scan:** Open BetterPaste and click "Rescan Directory". Select the files you want the AI to see.
2.  **Generate:** Click "Generate XML" -> "Copy to Clipboard".
3.  **Prompt:** Paste the XML into ChatGPT/Claude/Gemini. Ask your question.
4.  **Patch:** When the AI responds with code blocks, the Userscript detects them and sends them to BetterPaste.
5.  **Review:** Go to the "Patcher" tab in BetterPaste. You will see the incoming changes.
6.  **Apply:** Click "Apply" to update your files.

## 🔧 Supported AIs
*   ChatGPT (`chatgpt.com`)
*   Claude (`claude.ai`)
*   Google Gemini (`gemini.google.com`)
*   Google AI Studio (`aistudio.google.com`)
*   DeepSeek (`chat.deepseek.com`)

## 🏗️ Development

### Prerequisites
*   Rust (latest stable)
*   Visual Studio Build Tools (for C++ linker)

### Build
```bash
# Debug Build
cargo run

# Release Build (Optimized & Signed)
./build_release.ps1
