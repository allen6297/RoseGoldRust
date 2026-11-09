fn contains(s: String, sub: String): Bool {
    return __str.contains(s, sub);
}

fn starts_with(s: String, prefix: String): Bool {
    return __str.starts_with(s, prefix);
}

fn ends_with(s: String, suffix: String): Bool {
    return __str.ends_with(s, suffix);
}

fn length(s: String): Int {
    return __str.length(s);
}

fn is_empty(s: String): Bool {
    return __str.is_empty(s);
}

fn repeat(s: String, n: Int): String {
    return __str.repeat(s, n);
}

fn upper(s: String): String {
    return __str.upper(s);
}

fn lower(s: String): String {
    return __str.lower(s);
}

fn trim(s: String): String {
    return __str.trim(s);
}

fn slice(s: String, start: Int, end: Int): String {
    return __str.slice(s, start, end);
}

fn split(s: String, sep: String): Array[String] {
    return __str.split(s, sep);
}

fn replace(s: String, old: String, with: String): String {
    return __str.replace(s, old, with);
}

fn find(s: String, sub: String): Int {
    return __str.find(s, sub);
}
