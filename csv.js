function csvField(value) {
    if (value === null || value === undefined) { return ""; }
    var text = String(value);
    if (typeof value === "string" && /^[=+\-@\t\r]/.test(text)) { text = "\u0027" + text; }
    if (/[",\r\n]/.test(text)) { text = "\"" + text.replace(/"/g, "\"\"") + "\""; }
    return text;
}

function toCsv(columns, rows) {
    var lines = [columns.map(csvField).join(",")];
    rows.forEach(function (row) { lines.push(row.map(csvField).join(",")); });
    return lines.join("\r\n") + "\r\n";
}

function downloadCsv(columns, rows) {
    var blob = new Blob(["\ufeff" + toCsv(columns, rows)], { type: "text/csv;charset=utf-8" });
    var url = URL.createObjectURL(blob);
    var link = document.createElement("a");
    link.href = url;
    link.download = "result-" + Date.now() + ".csv";
    document.body.appendChild(link);
    link.click();
    link.remove();
    URL.revokeObjectURL(url);
}

if (typeof module !== "undefined") { module.exports = { csvField: csvField, toCsv: toCsv }; }
