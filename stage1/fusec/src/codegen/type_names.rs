pub fn split_generic_args(type_name: &str) -> Option<Vec<String>> {
    let start = type_name.find('<')?;
    let end = type_name.rfind('>')?;
    let inner = &type_name[start + 1..end];
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut current = String::new();
    for ch in inner.chars() {
        match ch {
            '<' => {
                depth += 1;
                current.push(ch);
            }
            '>' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if depth == 0 => {
                args.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        args.push(current.trim().to_string());
    }
    Some(args)
}

pub fn option_inner_type(type_name: &str) -> Option<String> {
    let args = split_generic_args(type_name)?;
    (type_name.trim().trim_end_matches('>').contains("Option") && args.len() == 1)
        .then(|| args[0].clone())
}

pub fn result_ok_type(type_name: &str) -> Option<String> {
    let args = split_generic_args(type_name)?;
    (type_name.trim().trim_end_matches('>').contains("Result") && args.len() == 2)
        .then(|| args[0].clone())
}

pub fn result_err_type(type_name: &str) -> Option<String> {
    let args = split_generic_args(type_name)?;
    (type_name.trim().trim_end_matches('>').contains("Result") && args.len() == 2)
        .then(|| args[1].clone())
}

pub fn chan_inner_type(type_name: &str) -> Option<String> {
    let args = split_generic_args(type_name)?;
    (type_name.trim().trim_end_matches('>').contains("Chan") && args.len() == 1)
        .then(|| args[0].clone())
}
