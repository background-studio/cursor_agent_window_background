use serde::Serialize;

use crate::models::{DisplaySettings, MediaKind};

const TEMPLATE: &str = r#"(config=>{
  const agent=__AGENT_PROBE__;
  if(!agent)return{installed:false};
  const STATE="__CURSOR_AGENT_BACKGROUND_STUDIO__";
  const previous=window[STATE];
  if(previous&&previous.revision===config.revision&&previous.isHealthy?.())return{installed:true,unchanged:true};
  try{previous&&previous.cleanup&&previous.cleanup();}catch(e){}
  document.body.setAttribute("data-cbg-cursor-agent-active","1");
  const style=document.createElement("style");
  style.id="cbg-cursor-agent-style";
  style.textContent=config.css;
  document.head.appendChild(style);
  const layer=document.createElement("div");
  layer.id="cbg-cursor-agent-layer";
  layer.setAttribute("aria-hidden","true");
  const media=document.createElement(config.mediaKind==="video"?"video":"img");
  media.id="cbg-cursor-agent-media";
  if(config.mediaKind==="video"){
    media.autoplay=true;media.loop=true;media.muted=!!config.display.videoMuted;
    media.defaultMuted=!!config.display.videoMuted;media.playsInline=true;
    media.playbackRate=Number(config.display.videoPlaybackRate)||1;
  }
  media.src=config.mediaUrl;
  const overlay=document.createElement("div");
  overlay.id="cbg-cursor-agent-overlay";
  layer.append(media,overlay);
  document.body.prepend(layer);
  const rgba=(opacity)=>"rgba(12,12,14,"+Number(opacity||0)+")";
  const dark=(value)=>/oklch\(|color\(srgb 0\.1|rgba\(0,\s*0,\s*0,\s*0\.[2-9]|rgba\(0,\s*0,\s*0,\s*1\)/.test(value||"");
  const originals=new Map();
  const properties=["background-color","background-image","box-shadow","backdrop-filter"];
  const paint=(el,opacity)=>{
    if(!originals.has(el))originals.set(el,properties.map(name=>[name,el.style.getPropertyValue(name),el.style.getPropertyPriority(name)]));
    el.setAttribute("data-cbg-cursor-agent","1");
    el.style.setProperty("background-color",rgba(opacity),"important");
    el.style.setProperty("background-image","none","important");
    el.style.setProperty("box-shadow","none","important");
    el.style.setProperty("backdrop-filter","none","important");
  };
  const visit=(el)=>{
    if(!el||el.nodeType!==1||el.id==="cbg-cursor-agent-layer"||el.id==="cbg-cursor-agent-style"||el.matches('[data-component="glass-in-app-menubar"]'))return;
    const rect=el.getBoundingClientRect();
    const plate=rect.width>160&&rect.height>48&&rect.bottom>0&&rect.top<innerHeight;
    const name=String(el.className||"");
    if(plate&&dark(getComputedStyle(el).backgroundColor)&&!/ui-button|ui-pill|ui-tab/.test(name)){
      const full=rect.width>innerWidth*0.9&&rect.height>innerHeight*0.9;
      const side=rect.x<8&&rect.width<360;
      paint(el,full?0:side?config.display.sidebarOpacity:config.display.surfaceOpacity);
    }
    for(const child of el.children)visit(child);
  };
  let frame=0;
  let controlsFrame=0;
  let restoreControls=null;
  let disposed=false;
  const repairedBars=new WeakSet();
  const repairWindowControls=()=>{
    const bar=document.querySelector('[data-component="glass-in-app-menubar"]');
    const overlay=navigator.windowControlsOverlay;
    if(!bar||!overlay?.visible||restoreControls||repairedBars.has(bar))return;
    const height=parseFloat(getComputedStyle(bar).height);
    const rect=bar.getBoundingClientRect();
    const controls=overlay.getTitlebarAreaRect();
    // The native overlay may retain a bad height across windows. Never use a
    // zoomed bounding rectangle as a CSS height or freeze the menu's layout.
    if(!(height>0&&height<128&&rect.height>0)||controls.height<=rect.height*2)return;
    repairedBars.add(bar);
    const saved=["height","min-height","max-height"].map(name=>[name,bar.style.getPropertyValue(name),bar.style.getPropertyPriority(name)]);
    restoreControls=()=>{
      for(const [name,value,priority] of saved){
        if(value)bar.style.setProperty(name,value,priority);else bar.style.removeProperty(name);
      }
      restoreControls=null;
    };
    for(const [name] of saved)bar.style.setProperty(name,(height+1)+"px","important");
    // Two frames allow Cursor's ResizeObserver to publish the bounded native
    // height, then publish the original height after restoring all properties.
    controlsFrame=requestAnimationFrame(()=>{
      controlsFrame=requestAnimationFrame(()=>{if(!disposed)restoreControls?.();});
    });
  };
  const observer=new MutationObserver(()=>{
    cancelAnimationFrame(frame);
    frame=requestAnimationFrame(()=>{if(!disposed){visit(document.body);repairWindowControls();}});
  });
  observer.observe(document.body,{childList:true,subtree:true});
  visit(document.body);
  repairWindowControls();
  const cleanup=()=>{
    disposed=true;
    observer.disconnect();
    cancelAnimationFrame(frame);
    cancelAnimationFrame(controlsFrame);
    restoreControls?.();
    style.remove();
    layer.remove();
    document.body.removeAttribute("data-cbg-cursor-agent-active");
    for(const [el,values] of originals){
      for(const [name,value,priority] of values){
        if(value)el.style.setProperty(name,value,priority);else el.style.removeProperty(name);
      }
      el.removeAttribute("data-cbg-cursor-agent");
    }
    originals.clear();
    if(window[STATE]&&window[STATE].revision===config.revision)delete window[STATE];
  };
  const isHealthy=()=>!disposed&&style.isConnected&&layer.isConnected&&document.body.hasAttribute("data-cbg-cursor-agent-active");
  window[STATE]={revision:config.revision,cleanup,isHealthy};
  return{installed:true};
})(__CONFIG_JSON__)"#;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScriptConfig<'a> {
    media_url: &'a str,
    media_kind: &'a str,
    revision: &'a str,
    css: String,
    display: &'a DisplaySettings,
}

pub fn agent_css(display: &DisplaySettings) -> String {
    format!(
        r#"
body[data-cbg-cursor-agent-active] {{
  --cbg-opacity: {opacity};
  --cbg-overlay-opacity: {overlay};
  --cbg-sidebar-opacity: {sidebar};
  --cbg-surface-opacity: {surface};
  --cbg-card-opacity: {card};
  --cbg-composer-opacity: {composer};
  --glass-chat-bubble-background: rgba(12, 12, 14, {composer});
}}
#cbg-cursor-agent-layer {{
  position: fixed; inset: 0; z-index: 0; pointer-events: none; overflow: hidden;
  opacity: var(--cbg-opacity);
}}
html:has(#background-cover-style) body[data-cbg-cursor-agent-active]::before {{
  content: none !important;
  display: none !important;
  background-image: none !important;
}}
#cbg-cursor-agent-media {{
  width: 100%; height: 100%; object-fit: {fit}; object-position: {x}% {y}%;
  transform: scale({scale}); transform-origin: center; filter: blur({blur}px);
}}
#cbg-cursor-agent-overlay {{
  position: absolute; inset: 0; background: {color}; opacity: var(--cbg-overlay-opacity);
}}
/* Cursor forwards this computed color to Electron's native titleBarOverlay.
   Keep it in RGBA: OKLCH is valid CSS but prevents native controls updating. */
body[data-cbg-cursor-agent-active] [data-component="glass-in-app-menubar"] {{
  background-color: rgba(12, 12, 14, 0) !important;
}}
body[data-cbg-cursor-agent-active] .agent-panel,
body[data-cbg-cursor-agent-active] .editor-panel-container,
body[data-cbg-cursor-agent-active] .ui-tray {{
  background: rgba(12, 12, 14, var(--cbg-surface-opacity)) !important;
  background-color: rgba(12, 12, 14, var(--cbg-surface-opacity)) !important;
  background-image: none !important;
  box-shadow: none !important;
  backdrop-filter: none !important;
}}
body[data-cbg-cursor-agent-active][data-cursor-glass-mode="true"] .composer-human-message.standalone-glass,
body[data-cbg-cursor-agent-active][data-cursor-glass-mode="true"] .composer-human-message-container .ui-prompt-input__container,
body[data-cbg-cursor-agent-active] .ui-prompt-input__container,
body[data-cbg-cursor-agent-active] .composer-human-message {{
  background: rgba(12, 12, 14, var(--cbg-composer-opacity)) !important;
  background-color: rgba(12, 12, 14, var(--cbg-composer-opacity)) !important;
  border-color: transparent !important;
  box-shadow: none !important;
}}
body[data-cbg-cursor-agent-active] .ui-markdown__inline-code,
body[data-cbg-cursor-agent-active] code,
body[data-cbg-cursor-agent-active] .ui-pill,
body[data-cbg-cursor-agent-active] .ui-tab-system-tab,
body[data-cbg-cursor-agent-active] .ui-sidebar-menu-button,
body[data-cbg-cursor-agent-active] .ui-sidebar-menu-button::before,
body[data-cbg-cursor-agent-active] .ui-icon-button:not([data-component="glass-in-app-menubar"] *),
body[data-cbg-cursor-agent-active] .ui-prompt-input-submit-button {{
  background: transparent !important;
  background-color: transparent !important;
  background-image: none !important;
  box-shadow: none !important;
  backdrop-filter: none !important;
}}
"#,
        opacity = display.opacity,
        overlay = display.overlay_opacity,
        sidebar = display.sidebar_opacity,
        surface = display.surface_opacity,
        card = display.card_opacity,
        composer = display.composer_opacity,
        fit = fit_css(&display.fit),
        x = display.position_x,
        y = display.position_y,
        scale = display.scale,
        blur = display.blur,
        color = display.overlay_color,
    )
}

fn fit_css(fit: &crate::models::FitMode) -> &'static str {
    match fit {
        crate::models::FitMode::Cover => "cover",
        crate::models::FitMode::Contain => "contain",
        crate::models::FitMode::Fill => "fill",
        crate::models::FitMode::Tile => "none",
    }
}

pub fn install_script(
    media_url: &str,
    kind: &MediaKind,
    display: &DisplaySettings,
    revision: &str,
) -> Result<String, String> {
    let config = serde_json::to_string(&ScriptConfig {
        media_url,
        media_kind: match kind {
            MediaKind::Image => "image",
            MediaKind::Video => "video",
        },
        revision,
        css: agent_css(display),
        display,
    })
    .map_err(|error| error.to_string())?
    .replace('<', "\\u003c");
    Ok(TEMPLATE
        .replace("__AGENT_PROBE__", AGENT_PROBE)
        .replace("__CONFIG_JSON__", &config))
}

pub const AGENT_PROBE: &str = "Boolean(document.body&&document.querySelector('.agent-panel')&&(document.body.classList.contains('bc-window')||document.querySelector('[data-component=\"glass-in-app-menubar\"]')))";

pub const CLEANUP_SCRIPT: &str = r#"(()=>{const state=window.__CURSOR_AGENT_BACKGROUND_STUDIO__;if(state&&state.cleanup)state.cleanup();return true;})()"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::process::{Command, Stdio};

    #[test]
    fn javascript_window_lifecycle_regressions() {
        let script = install_script(
            "data:image/png;base64,AAAA",
            &MediaKind::Image,
            &DisplaySettings::default(),
            "test-revision",
        )
        .unwrap();
        let mut child = Command::new("node")
            .arg("tests/payload-runtime.cjs")
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Node.js is required for payload regression tests");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(
                serde_json::json!({ "script": script, "probe": AGENT_PROBE })
                    .to_string()
                    .as_bytes(),
            )
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn script_guards_editor_windows_and_covers_verified_selectors() {
        let script = install_script(
            "data:image/png;base64,aaaa",
            &MediaKind::Image,
            &DisplaySettings::default(),
            "rev",
        )
        .unwrap();
        assert!(script.contains("bc-window"));
        assert!(script.contains(".agent-panel"));
        assert!(script.contains(".editor-panel-container"));
        assert!(script.contains(".ui-tray"));
        assert!(script.contains("--glass-chat-bubble-background"));
        assert!(script.contains("glass-in-app-menubar"));
        assert!(
            script.contains("#background-cover-style) body[data-cbg-cursor-agent-active]::before")
        );
        assert!(script.contains(AGENT_PROBE));
        assert!(!script.contains("__AGENT_PROBE__"));
        assert!(!agent_css(&DisplaySettings::default()).contains("bc-window"));
        assert!(script.contains("installed:false"));
    }

    #[test]
    fn native_menubar_color_is_supported_without_overriding_layout() {
        let css = agent_css(&DisplaySettings::default());
        let rule = css
            .split("body[data-cbg-cursor-agent-active] [data-component=\"glass-in-app-menubar\"] {")
            .nth(1)
            .unwrap()
            .split('}')
            .next()
            .unwrap();
        assert!(rule.contains("background-color: rgba(12, 12, 14, 0) !important"));
        assert!(!rule.contains("height"));
        assert!(!rule.contains("display"));
        assert!(!rule.contains("position"));
    }
}
