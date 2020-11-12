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

async function oneshotChange(method, endpoint, error, obj) {
    const resp = await fetchJson(endpoint, obj, {method});
    if (resp.status === 200) {
        location.reload();
    } else {
        displayError(error);
    }
}

async function inputModal(text) {
    // TODO: make this pretty
    return prompt(text);
}

async function confirmModal(text) {
    // TODO: make this pretty
    return confirm(text);
}

function displayError(e) {
    // TODO: make this pretty
    alert(e);
}


window.addEventListener("load", () => {
    for (const sensor of document.querySelectorAll(".sensor")) {
        const addr = sensor.querySelector(".addr").textContent.trim();
        const labelNode = sensor.querySelector(".label");
        const label = labelNode.textContent.trim();
        labelNode.addEventListener("click", async () => {
            const newLabel = await inputModal(`Change label for ${addr}`, label);
            if (newLabel !== null) {
                oneshotChange("PUT", "/api/change_label", "Could not change label", {
                    addr,
                    new_label: newLabel,
                });
            }
        });
        sensor.querySelector(".forget").addEventListener("click", async () => {
            if (await confirmModal(`Are you sure you want to forget sensor ${addr}?`)) {
                oneshotChange("DELETE", "/api/forget", `Failed deleting ${addr}`, {
                    addr,
                });
            }
        });
    }
});
