use crate::{
    app::Application,
    core::{Result, timestamp},
};
use serde_json::{Value, json};
pub fn redact(value: &mut Value) {
    match value {
        Value::Object(o) => {
            for (k, v) in o {
                let key = k.to_ascii_lowercase();
                if [
                    "password",
                    "secret",
                    "token",
                    "private",
                    "credential",
                    "env",
                ]
                .iter()
                .any(|s| key.contains(s))
                {
                    *v = Value::String("[REDACTED]".into());
                } else {
                    redact(v);
                }
            }
        }
        Value::Array(a) => {
            for v in a {
                redact(v);
            }
        }
        Value::String(s) if s.contains("PRIVATE KEY") => {
            *value = Value::String("[REDACTED]".into());
        }
        _ => {}
    }
}
pub fn report(app: &mut Application) -> Result<String> {
    let services = app.supervisor.states(&app.store)?;
    let mut data = json!({"created_at":timestamp(),"warning":"Contains local paths. Review before sharing; no upload is performed.","home":app.home.root(),"platform":crate::platform::platform_key(),"dns":{"resolver":app.dns_owned(),"policy":crate::platform::wildcard_ready(),"system":crate::platform::wildcard_system_ready()},"ca":crate::setup::state(&app.store,&app.home)?,"services":services,"runtimes":crate::runtime::installed(&app.store)?.iter().map(|r|json!({"id":r.id,"path":r.relative_path,"binary_exists":r.binary(&app.home,"cli").or_else(|_|r.binary(&app.home,"server")).is_ok_and(|p|p.is_file())})).collect::<Vec<_>>(),"tools":crate::tools::installed(&app.store)?.iter().map(|t|json!({"id":t.id,"version":t.version})).collect::<Vec<_>>(),"ports":[]});
    let mut stmt = app
        .store
        .conn
        .prepare("SELECT owner,port FROM port_allocations ORDER BY owner")?;
    data["ports"]=Value::Array(stmt.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,u16>(1)?)))?.collect::<std::result::Result<Vec<_>,_>>()?.into_iter().map(|(owner,port)|json!({"owner":owner,"port":port,"listening":crate::ports::PortManager::healthy(port)})).collect());
    redact(&mut data);
    Ok(serde_json::to_string_pretty(&data)?)
}
