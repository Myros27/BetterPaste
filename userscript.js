// ==UserScript==
// @name         BetterPaste Connector
// @namespace    http://tampermonkey.net/
// @version      1.4
// @description  Scans AI chat for BetterPaste code blocks
// @match        https://chatgpt.com/*
// @match        https://gemini.google.com/*
// @match        https://claude.ai/*
// @match        https://chat.deepseek.com/*
// @match        https://aistudio.google.com/*
// @connect      127.0.0.1
// @grant        GM_xmlhttpRequest
// @run-at       document-idle
// ==/UserScript==

(function() {
    'use strict';

    const SERVER_URL = "http://127.0.0.1:3030/api/diff";
    const SCAN_INTERVAL_MS = 1000;

    let isScanning = false; // Start Paused
    let cornerIndex = 0; // 0=BR, 1=BL, 2=TL, 3=TR

    const uiContainer = document.createElement('div');
    uiContainer.style.cssText = 'position:fixed; z-index:9999; display:flex; align-items:center; gap:8px; padding:6px 10px; background:#222; border:1px solid #444; color:#fff; border-radius:6px; font-family:sans-serif; font-size:12px; box-shadow:0 4px 6px rgba(0,0,0,0.3); transition:all 0.3s ease;';

    const statusText = document.createElement('span');
    statusText.innerText = "BP: Paused";
    statusText.style.fontWeight = "bold";
    statusText.style.minWidth = "70px";

    const toggleBtn = document.createElement('button');
    toggleBtn.innerText = "▶";
    toggleBtn.style.cssText = 'background:#444; color:white; border:none; padding:4px 8px; border-radius:4px; cursor:pointer; font-size:12px;';

    const moveBtn = document.createElement('button');
    moveBtn.innerText = "✥";
    moveBtn.style.cssText = 'background:#444; color:white; border:none; padding:4px 8px; border-radius:4px; cursor:pointer; font-size:12px;';

    uiContainer.appendChild(statusText);
    uiContainer.appendChild(toggleBtn);
    uiContainer.appendChild(moveBtn);
    document.body.appendChild(uiContainer);

    const applyPosition = () => {
        uiContainer.style.top = uiContainer.style.bottom = uiContainer.style.left = uiContainer.style.right = 'auto';
        const margin = '15px';
        if (cornerIndex === 0) { uiContainer.style.bottom = margin; uiContainer.style.right = margin; }
        else if (cornerIndex === 1) { uiContainer.style.bottom = margin; uiContainer.style.left = margin; }
        else if (cornerIndex === 2) { uiContainer.style.top = margin; uiContainer.style.left = margin; }
        else if (cornerIndex === 3) { uiContainer.style.top = margin; uiContainer.style.right = margin; }
    };
    applyPosition();

    toggleBtn.onclick = () => {
        isScanning = !isScanning;
        if (isScanning) {
            toggleBtn.innerText = "⏸";
            statusText.innerText = "BP: Idle";
            statusText.style.color = " #fff";
            scanForBlocks();
        } else {
            toggleBtn.innerText = "▶";
            statusText.innerText = "BP: Paused";
            statusText.style.color = " #aaa";
        }
    };

    moveBtn.onclick = () => { cornerIndex = (cornerIndex + 1) % 4; applyPosition(); };

    const BLOCK_REGEX = /\[<\(x\{START\}x\)>\]\s*([\s\S]*?)\s*\[<\(x\{SEARCH\}x\)>\]\s*([\s\S]*?)\s*\[<\(x\{REPLACEWITH\}x\)>\]\s*([\s\S]*?)\s*\[<\(x\{END\}x\)>\]/g;

    function updateStatus(msg, color = null) {
        if (!isScanning) return;
        statusText.innerText = msg;
        if (color) statusText.style.color = color;
    }

    function scanForBlocks() {
        if (!isScanning) return;
        const bodyText = document.body.innerText;

        BLOCK_REGEX.lastIndex = 0;
        let match;

        while ((match = BLOCK_REGEX.exec(bodyText)) !== null) {
            const fullMatch = match[0];
            const filePath = match[1].trim();
            const searchBlock = match[2];
            const replaceBlock = match[3];
            const normalizedContent = fullMatch.replace(/\s/g, '');
            const blockHash = cyrb53(normalizedContent);

            if (sessionStorage.getItem(`bp_sent_${blockHash}`)) continue;

            if (searchBlock.length > 60 && !searchBlock.includes('\n')) {
                console.warn(`[BetterPaste] Skipping suspicious flattened block for ${filePath}`);
                continue;
            }

            updateStatus(`Sending...`, ' #e67e22');

            const payload = JSON.stringify({
                file_path: filePath,
                search_content: searchBlock,
                replace_content: replaceBlock
            });

            GM_xmlhttpRequest({
                method: "POST",
                url: SERVER_URL,
                headers: { "Content-Type": "application/json" },
                data: payload,
                onload: function(res) {
                    if (res.status >= 200 && res.status < 300) {
                        sessionStorage.setItem(`bp_sent_${blockHash}`, "true");
                        updateStatus("Synced", '#27ae60');
                        setTimeout(() => updateStatus("BP: Idle", ' #fff'), 2000);
                    } else {
                        updateStatus("Err: Backend", ' #c0392b');
                    }
                },
                onerror: function() {
                    updateStatus("Err: Connect", ' #c0392b');
                }
            });
        }
    }

    const cyrb53 = function(str, seed = 0) {
        let h1 = 0xdeadbeef ^ seed, h2 = 0x41c6ce57 ^ seed;
        for (let i = 0, ch; i < str.length; i++) {
            ch = str.charCodeAt(i);
            h1 = Math.imul(h1 ^ ch, 2654435761);
            h2 = Math.imul(h2 ^ ch, 1597334677);
        }
        h1 = Math.imul(h1 ^ (h1 >>> 16), 2246822507) ^ Math.imul(h2 ^ (h2 >>> 13), 3266489909);
        h2 = Math.imul(h2 ^ (h2 >>> 16), 2246822507) ^ Math.imul(h1 ^ (h1 >>> 13), 3266489909);
        return 4294967296 * (2097151 & h2) + (h1 >>> 0);
    };

    setInterval(scanForBlocks, SCAN_INTERVAL_MS);
})();
