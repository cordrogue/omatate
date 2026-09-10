export function defaults() {
    return {save_note:["Ctrl+Return","Ctrl+S"],delete_note:["Ctrl+Delete"],focus_note:["Ctrl+N"],search:["Ctrl+F"],
        projects:["Ctrl+P"],new_project:["Ctrl+Shift+N"],open_folder:["Ctrl+O"],add_section:["Ctrl+Shift+S"],clip:["Ctrl+Shift+C"],
        toggle_ai:["Ctrl+Shift+A"],cycle_opacity:["Ctrl+Shift+O"],minimize:["Ctrl+M"],end_session:["Ctrl+Shift+W"],
        previous_entry:["Alt+Up"],next_entry:["Alt+Down"],preview:["Alt+Return"],expand_note:["Shift+Alt+Return"],help:["Ctrl+H","F1"]}
}
// The documented keys.toml format is a [keys] table of string arrays, including
// multiline arrays and comments. Reject unsupported input rather than guessing.
export function keyTable(text) {
    var tokens = text.match(/"(?:\\.|[^"\\])*"|'[^']*'|#[^\n]*|[A-Za-z_][A-Za-z_0-9-]*|[\[\]=,]|\s+|./g) || []
    tokens = tokens.filter(t => !/^\s+$/.test(t) && t[0] !== "#")
    var pos = 0, out = {}, table = false
    function take(expected) { if (tokens[pos++] !== expected) throw new Error("Malformed keyboard config") }
    while (pos < tokens.length) {
        if (tokens[pos] === "[") {
            if (table) throw new Error("Duplicate keys table")
            take("["); take("keys"); take("]"); table = true; continue
        }
        if (!table) throw new Error("Expected [keys] table")
        var name = tokens[pos++]
        if (!/^[a-z_]+$/.test(name) || Object.prototype.hasOwnProperty.call(out, name)) throw new Error("Invalid or duplicate keyboard action")
        take("="); take("[")
        var values = []
        while (tokens[pos] !== "]") {
            var value = tokens[pos++]
            if (!value || (value[0] !== "'" && value[0] !== '"')) throw new Error("Bindings must contain strings")
            values.push(value[0] === "'" ? value.slice(1, -1) : JSON.parse(value))
            if (tokens[pos] !== "]") take(",")
        }
        take("]"); out[name] = values
    }
    return out
}
export function accelerator(text) {
    var parts = text.replace(/<([^>]+)>/g, "$1+").split("+"), key = parts.pop(), mods = {}
    var aliases = {control:"Ctrl",ctrl:"Ctrl",primary:"Ctrl",shift:"Shift",alt:"Alt",mod1:"Alt",super:"Meta",meta:"Meta"}
    parts.forEach(function(part) { var mod = aliases[part.toLowerCase()]; if (!mod || mods[mod]) throw new Error("Invalid modifier " + part); mods[mod] = true })
    var names = {return:"Return",enter:"Return",kp_enter:"Return",up:"Up",down:"Down",left:"Left",right:"Right",home:"Home",end:"End",
        page_up:"PgUp",pageup:"PgUp",pgup:"PgUp",page_down:"PgDown",pagedown:"PgDown",pgdown:"PgDown",backspace:"Backspace",delete:"Delete",space:"Space"}
    var lower = key.toLowerCase()
    if (names[lower]) key = names[lower]
    else if (/^f([1-9]|[12][0-9]|3[0-5])$/.test(lower) || /^[a-z0-9,.;/\[\]'=\x60-]$/i.test(key)) key = key.toUpperCase()
    else throw new Error("Unsupported or reserved key " + key)
    var count = Object.keys(mods).length
    if ((!count && !/^F\d+$/.test(key)) || (count === 1 && mods.Shift && key !== "Return")) throw new Error("Bare keys are reserved for typing")
    return ["Ctrl","Shift","Alt","Meta"].filter(mod => mods[mod]).concat([key]).join("+")
}
export function keys(text) {
    var result = defaults()
    var table = keyTable(text || "")
    Object.keys(table).forEach(function(action) {
        if (!Object.prototype.hasOwnProperty.call(result, action)) throw new Error("Unknown keyboard action " + action)
        result[action] = table[action].map(accelerator)
    })
    var used = {}
    Object.keys(result).forEach(action => result[action].forEach(function(key) {
        if (used[key]) throw new Error("Duplicate binding " + key + " for " + used[key] + " and " + action)
        used[key] = action
    }))
    return result
}
