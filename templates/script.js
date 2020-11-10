function fetchJson(endpoint, body, options = {}) {
    const opt = {
        method: "GET",
        headers: {
            "Content-Type": "application/json",
        },
        body: JSON.stringify(body),
        ... options
    };
    return fetch(endpoint, opt);
}

function displayError(e) {
    // TODO: make this nice
    alert(e);
}

async function showChangeModal(addr, oldLabel) {
    // TODO: make this nice
    const label = prompt(`Change label for ${addr}`, oldLabel);
    if (label !== null) {
        const req = {
            addr,
            new_label: label,
        };
        const resp = await fetchJson("/api/change_label", req, {method: "PUT"});
        if (resp.status === 200) {
            location.reload();
        } else {
            displayError("Failed changing label");
        }
    }
}

window.addEventListener("load", () => {
    for (const sensor of document.querySelectorAll(".sensor")) {
        const addr = sensor.querySelector(".addr").textContent.trim();
        const label = sensor.querySelector(".label").textContent.trim();
        sensor.querySelector(".label").addEventListener("click", () => {
            showChangeModal(addr, label);
        });
    }
});
