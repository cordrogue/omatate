// Project records and Markdown rendering. No UI or filesystem dependencies.
export function parseEntries(text) {
    var ids = {}
    return text.split(/\r?\n/).filter(line => line.trim()).map(function(line) {
        var entry = JSON.parse(line)
        if (!entry || typeof entry !== "object" || Array.isArray(entry)
                || !Number.isSafeInteger(entry.id) || entry.id < 1 || ids[entry.id])
            throw new Error("Entries must have unique positive integer ids")
        ids[entry.id] = true
        return entry
    })
}
export function encodeEntries(entries) { return entries.map(entry => JSON.stringify(entry) + "\n").join("") }
export function nextId(entries) {
    var id = entries.reduce((max, entry) => Math.max(max, entry.id), 0) + 1
    if (!Number.isSafeInteger(id)) throw new Error("Entry ids exhausted")
    return id
}
export function clone(value) { return JSON.parse(JSON.stringify(value)) }
export function timestamp(date) {
    date = date || new Date()
    function pad(n) { return String(n).padStart(2, "0") }
    return date.getFullYear() + "-" + pad(date.getMonth() + 1) + "-" + pad(date.getDate())
        + "T" + pad(date.getHours()) + ":" + pad(date.getMinutes()) + ":" + pad(date.getSeconds())
}
export function filenameStamp() { return timestamp().replace(/[-:]/g, "").replace("T", "-") }
export function validName(name) { return !!name && name.trim() === name && name !== "." && name !== ".." && !/[\/\\\x00]/.test(name) }
export function render(path, entries, today) {
    var name = path.split("/").pop(), label = name, date = today || timestamp().slice(0, 10)
    var match = /^(\d{4})(\d{2})(\d{2})-\d{6}(?:-(.*))?$/.exec(name)
    if (match) { date = match[1] + "-" + match[2] + "-" + match[3]; label = match[4] || name }
    var out = "# UI review — " + label + " · " + date + "\n\n"
    var sections = entries.some(entry => entry.kind === "section")
    entries.slice().sort((a, b) => a.id - b.id).forEach(function(entry) {
        if (entry.kind === "section") { out += "## " + (entry.title || "") + "\n\n"; return }
        var comment = (entry.transcript || "").trim()
            || (["recording", "transcribing"].indexOf(entry.status) >= 0 ? "_transcribing…_" : "_no transcript_")
        if (entry.asset) out += "![Screen capture](" + entry.asset + ")\n\n"
        if (entry.ai === false) { out += comment + "\n\n"; return }
        var context = entry.context || {}
        out += (sections ? "### " : "## ") + entry.id + ". " + (context.title || "Entry " + entry.id) + "\n"
        out += typeof context.summary === "string" ? context.summary : (entry.context_status === "error" ? "_analysis failed_" : "_analysis pending…_")
        out += "\n\n> " + comment.replace(/\n/g, "\n> ") + "\n\n"
    })
    return out.replace(/\n+$/, "\n")
}
export function mutate(entries, command, request) {
    entries = clone(entries)
    var entry = entries.find(item => item.id === request.entryId)
    if (command === "note") {
        if (typeof request.text !== "string" || !request.text.trim()) throw new Error("Note is empty")
        entries.push({id: nextId(entries), ts: timestamp(), ai: false, status: "done", transcript: request.text})
        return entries
    }
    if (!entry) throw new Error("Entry disappeared")
    if (["recording", "transcribing"].indexOf(entry.status) >= 0) throw new Error("Finish transcription before editing this entry")
    if (command === "edit") {
        if (typeof request.text !== "string") throw new Error("Missing note text")
        entry[entry.kind === "section" ? "title" : "transcript"] = request.text
    } else if (command === "delete-section") {
        if (entry.kind !== "section") throw new Error("Only section headings can be deleted")
        entries = entries.filter(item => item.id !== entry.id)
    } else if (command === "delete-note") {
        if (entry.kind === "section") throw new Error("Section headings must be deleted separately")
        if (entry.context_status === "pending") throw new Error("Finish analysis before deleting this entry")
        entries = entries.filter(item => item.id !== entry.id)
    } else throw new Error("Unknown mutation")
    return entries
}
export function context(text) {
    var parsed
    try { parsed = JSON.parse(text) } catch (error) {
        var start = text.indexOf("{"), end = text.lastIndexOf("}")
        if (start < 0) throw new Error("Could not parse analysis output")
        parsed = JSON.parse(text.slice(start, end + 1))
    }
    if (!parsed || Object.keys(parsed).sort().join() !== "notable,regions,summary,title"
            || typeof parsed.title !== "string" || typeof parsed.summary !== "string"
            || !Array.isArray(parsed.notable) || !parsed.notable.every(x => typeof x === "string")
            || !Array.isArray(parsed.regions) || !parsed.regions.every(x => x && Object.keys(x).sort().join() === "contents,name"
                && typeof x.name === "string" && typeof x.contents === "string"))
        throw new Error("Invalid analysis context")
    return parsed
}
