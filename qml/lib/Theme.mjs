function valid(color) { return /^#(?:[0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i.test(color || '') }
function rgb(color) {
    var value=color.slice(1)
    if (value.length===3) value=value.split('').map(c=>c+c).join('')
    return [0,2,4].map(i=>parseInt(value.slice(i,i+2),16))
}
function hex(n) { return Math.round(n).toString(16).padStart(2,'0') }
function mix(a,b,n) { var aa=rgb(a),bb=rgb(b);return '#'+aa.map((v,i)=>hex(v+(bb[i]-v)*n)).join('') }
function luminance(c) { var v=rgb(c).map(x=>x/255).map(x=>x<=0.04045?x/12.92:Math.pow((x+0.055)/1.055,2.4));return .2126*v[0]+.7152*v[1]+.0722*v[2] }
function contrast(a,b) { var aa=luminance(a),bb=luminance(b);return (Math.max(aa,bb)+.05)/(Math.min(aa,bb)+.05) }
function qt(c) { return c.length===9?'#'+c.slice(7)+c.slice(1,7):c }
function settings(text) {
    var result={}
    text.split(/\r?\n/).forEach(function(line) { var i=line.indexOf('=');if(i>=0) result[line.slice(0,i).trim()]=line.slice(i+1).trim() })
    return result
}
export function sources(home, config) {
    return [home+'/.local/state/omarchy/current/theme/colors.toml',home+'/.config/omarchy/current/theme/colors.toml',
        home+'/.local/state/omarchy/current/theme/ghostty.conf',home+'/.config/omarchy/current/theme/ghostty.conf',
        config+'/ghostty/config',config+'/ghostty/config.ghostty',home+'/.config/omarchy/shell.toml',home+'/.config/fontconfig/fonts.conf']
}
export function configSources(text, home) {
    return text.split(/\r?\n/).filter(line=>/^config-file\s*=/.test(line)).map(function(line) {
        var path=line.slice(line.indexOf('=')+1).trim().replace(/^\?/,'')
        return path.indexOf('~/')===0?home+path.slice(1):path
    }).filter(path=>path[0]==='/')
}
export function resolve(paletteText, shellText, ghosttyText, font) {
    var colors={background:'#3a332a',foreground:'#f5ecd9',muted:'#968872',accent:'#d6a06b',red:'#ed806b',yellow:'#dab46b',green:'#83baa1'}
    var palette={}
    paletteText.split(/\r?\n/).forEach(function(line) {
        var match=/^\s*([\w-]+)\s*=\s*['"](#[0-9a-f]+)['"]/i.exec(line)
        if(match&&valid(match[2])) palette[match[1]]=match[2]
    })
    if(valid(palette.background)&&valid(palette.foreground)) {
        Object.assign(colors,palette)
        colors.accent=palette.accent||palette.blue||palette.cyan||palette.green||palette.foreground
        colors.muted=palette.muted||palette.light_foreground||palette.dark_foreground||palette.foreground
        colors.red=palette.red||palette.bright_red||colors.accent
        colors.yellow=palette.yellow||palette.bright_yellow||colors.accent
        colors.green=palette.green||palette.bright_green||colors.accent
    }
    var size=11, match=/base-size\s*=\s*([\d.]+)/.exec(shellText)
    if(match&&+match[1]>=6&&+match[1]<=40) size=+match[1]
    var cfg=settings(ghosttyText), terminal=valid(cfg.background)&&valid(cfg.foreground)
    if(terminal) {
        colors.background=cfg.background;colors.foreground=cfg.foreground
        ghosttyText.split(/\r?\n/).forEach(function(line) {
            var m=/^palette\s*=\s*(\d+)\s*=\s*(#[0-9a-f]+)/i.exec(line)
            var roles={1:'red',2:'green',3:'yellow',4:'accent',8:'muted'}
            // ANSI blue is only an accent fallback; the theme names its own accent.
            if(m&&roles[m[1]]&&valid(m[2])&&!(m[1]==='4'&&valid(palette.accent))) colors[roles[m[1]]]=m[2]
        })
        if(+cfg['font-size']>=6&&+cfg['font-size']<=40) size=+cfg['font-size']
        font=cfg['font-family']||font
    }
    var bg=colors.background,fg=colors.foreground,muted=colors.muted
    var edge=luminance(bg)<luminance(fg)?'#000000':'#ffffff'
    var surfaces=terminal?[bg]:[bg,colors.lighter_background||mix(bg,fg,.1),colors.darker_background||mix(bg,edge,.28)]
    for(var i=0;i<=20;i++) { muted=mix(colors.muted,fg,i/20);if(surfaces.every(s=>contrast(muted,s)>=4.5)) break }
    return {background:qt(bg),control:mix(bg,'#000000',.05),foreground:qt(fg),muted:qt(muted),accent:qt(colors.accent),
        red:qt(colors.red),yellow:qt(colors.yellow),green:qt(colors.green),hair:'#40'+mix(fg,fg,0).slice(1),
        selectionBg:terminal?qt(valid(cfg['selection-background'])?cfg['selection-background']:colors.accent):'#73'+mix(colors.accent,colors.accent,0).slice(1),
        selectionFg:qt(terminal&&valid(cfg['selection-foreground'])?cfg['selection-foreground']:fg),
        cursor:qt(terminal&&valid(cfg['cursor-color'])?cfg['cursor-color']:colors.accent),fontFamily:(font||'JetBrainsMono Nerd Font').trim().replace(/[\r\n]/g,' '),
        fontPointSize:size,smallPointSize:terminal?size:Math.max(size*.82,Math.min(size,8))}
}
