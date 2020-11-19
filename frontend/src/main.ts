import "./css/style.css";
import { Chart } from "chart.js";

function fetchJson(endpoint, body = null, options = {}) {
    const opt = {
        method: "GET",
        headers: {
            "Content-Type": "application/json",
        },
        body: body ? JSON.stringify(body) : null,
        ...options,
    };
    return fetch(endpoint, opt);
}

async function oneshotChange(
    method: string,
    endpoint: string,
    error: string,
    obj: any
) {
    const resp = await fetchJson(endpoint, obj, { method });
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

function overview() {
    for (const sensor of document.querySelectorAll(".sensor")) {
        const addr = sensor.querySelector(".addr").textContent.trim();
        const labelNode = sensor.querySelector(".label");
        const label = labelNode.textContent.trim();
        labelNode.addEventListener("click", async () => {
            const newLabel = await inputModal(`Change label for ${addr}`);
            if (newLabel !== null) {
                oneshotChange(
                    "PUT",
                    "/api/change_label",
                    "Could not change label",
                    {
                        addr,
                        new_label: newLabel,
                    }
                );
            }
        });
        sensor.querySelector(".forget").addEventListener("click", async () => {
            if (
                await confirmModal(
                    `Are you sure you want to forget sensor ${addr}?`
                )
            ) {
                oneshotChange(
                    "DELETE",
                    "/api/forget",
                    `Failed deleting ${addr}`,
                    {
                        addr,
                    }
                );
            }
        });
    }
}

function format(n: number, precision: number, unit: string): string {
    if (precision < 0) {
        throw `Format received invalid precision ${precision}`;
    }

    if (precision === 0) {
        return `${n}${unit}`;
    } else {
        const div = 10 ** precision;
        const rest = `${n % div}`.padStart(precision, "0");
        return `${Math.floor(n / div)}.${rest}${unit}`;
    }
}

async function detail() {
    const addr = document.querySelector(".addr").textContent.trim();
    const req = await fetchJson(`/api/log/${addr}`);
    const values = await req.json();
    const canvas = document.getElementById("chart") as HTMLCanvasElement;
    console.log(values);
    const ctx = canvas.getContext("2d");
    new Chart(ctx, {
        type: "scatter",
        data: {
            datasets: [
                {
                    label: "Temperature",
                    data: values.map((v) => {
                        return { x: v.time, y: v.values.temperature };
                    }),
                    borderWidth: 1,
                },
            ],
        },
        options: {
            scales: {
                xAxes: [
                    {
                        ticks: {
                            callback: (timestamp) => {
                                return new Date(timestamp * 1000).toISOString();
                            },
                        },
                    },
                ],
                yAxes: [
                    {
                        ticks: {
                            callback: (temperature) => {
                                return format(temperature, 2, "°C");
                            },
                        },
                    },
                ],
            },
        },
    });
}

window.addEventListener("load", () => {
    const view = document.querySelector("body")?.id;
    if (view) {
        // FIXME: terrible!
        eval(`${view}()`);
    }
});
