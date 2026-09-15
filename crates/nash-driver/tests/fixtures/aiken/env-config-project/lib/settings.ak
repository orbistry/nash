use env
use config
pub fn selected() -> Bool {
 let (number, flag) = config.mixed
 config.limit == env.expected && number > 0 && (flag || config.enabled) && config.bytes != #"" && config.values != []
}
