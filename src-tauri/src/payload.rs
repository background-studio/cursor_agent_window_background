use serde::Serialize;

use crate::models::{DisplaySettings, MediaKind};

const TEMPLATE: &str = r#"(config=>{
  const agent=document.body&&document.body.classList.contains("bc-window")&&document.querySelector(".agent-panel");
  if(!agent)return{installed:false};
  const STATE="__CURSOR_AGENT_BACKGROUND_STUDIO__";
  const previous=window[STATE];
  if(previous&&previous.revision===config.revision)return{installed:true,unchanged:true};
  try{previous&&previous.cleanup&&previous.cleanup();}catch(e){}
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
  const paint=(el,opacity)=>{
    el.setAttribute("data-cbg-cursor-agent","1");
    el.style.setProperty("background-color",rgba(opacity),"important");
    el.style.setProperty("background-image","none","important");
    el.style.setProperty("box-shadow","none","important");
    el.style.setProperty("backdrop-filter","none","important");
  };
  const visit=(el)=>{
    if(!el||el.nodeType!==1||el.id==="cbg-cursor-agent-layer"||el.id==="cbg-cursor-agent-style")return;
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
  const observer=new MutationObserver(()=>{
    cancelAnimationFrame(frame);
    frame=requestAnimationFrame(()=>visit(document.body));
  });
  observer.observe(document.body,{childList:true,subtree:true});
  visit(document.body);
  const bar=document.querySelector('[data-component="glass-in-app-menubar"]');
  if(bar){
    bar.style.setProperty("background-color","rgba(0,0,0,0)","important");
    const height=Math.round(bar.getBoundingClientRect().height)||36;
    bar.style.height=(height+6)+"px";
    requestAnimationFrame(()=>{bar.style.height=height+"px";});
  }
  const cleanup=()=>{
    observer.disconnect();
    style.remove();
    layer.remove();
    document.querySelectorAll("[data-cbg-cursor-agent]").forEach((el)=>{
      el.style.removeProperty("background-color");
      el.style.removeProperty("background-image");
      el.style.removeProperty("box-shadow");
      el.style.removeProperty("backdrop-filter");
      el.removeAttribute("data-cbg-cursor-agent");
    });
    if(window[STATE]&&window[STATE].revision===config.revision)delete window[STATE];
  };
  window[STATE]={revision:config.revision,cleanup};
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
body.bc-window {{
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
#cbg-cursor-agent-media {{
  width: 100%; height: 100%; object-fit: {fit}; object-position: {x}% {y}%;
  transform: scale({scale}); transform-origin: center; filter: blur({blur}px);
}}
#cbg-cursor-agent-overlay {{
  position: absolute; inset: 0; background: {color}; opacity: var(--cbg-overlay-opacity);
}}
body.bc-window .agent-panel,
body.bc-window .editor-panel-container,
body.bc-window .ui-tray {{
  background: rgba(12, 12, 14, var(--cbg-surface-opacity)) !important;
  background-color: rgba(12, 12, 14, var(--cbg-surface-opacity)) !important;
  background-image: none !important;
  box-shadow: none !important;
  backdrop-filter: none !important;
}}
body.bc-window[data-cursor-glass-mode="true"] .composer-human-message.standalone-glass,
body[data-cursor-glass-mode="true"].bc-window .composer-human-message.standalone-glass,
body.bc-window[data-cursor-glass-mode="true"] .composer-human-message-container .ui-prompt-input__container,
body.bc-window .ui-prompt-input__container,
body.bc-window .composer-human-message {{
  background: rgba(12, 12, 14, var(--cbg-composer-opacity)) !important;
  background-color: rgba(12, 12, 14, var(--cbg-composer-opacity)) !important;
  border-color: transparent !important;
  box-shadow: none !important;
}}
body.bc-window .ui-markdown__inline-code,
body.bc-window code,
body.bc-window .ui-pill,
body.bc-window .ui-tab-system-tab,
body.bc-window .ui-sidebar-menu-button,
body.bc-window .ui-sidebar-menu-button::before,
body.bc-window .ui-icon-button,
body.bc-window .ui-prompt-input-submit-button,
body.bc-window [data-component="glass-in-app-menubar"] {{
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
    Ok(TEMPLATE.replace("__CONFIG_JSON__", &config))
}

pub const AGENT_PROBE: &str = "Boolean(document.body&&document.body.classList.contains('bc-window')&&document.querySelector('.agent-panel'))";

pub const CLEANUP_SCRIPT: &str = r#"(()=>{const state=window.__CURSOR_AGENT_BACKGROUND_STUDIO__;if(state&&state.cleanup)state.cleanup();return true;})()"#;

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(script.contains("installed:false"));
    }
}
