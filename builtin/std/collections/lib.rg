fn contains[T](xs: Array[T], item: T): Bool {
    for x in xs {
        if x == item {
            return true;
        }
    }
    return false;
}

fn index_of[T](xs: Array[T], item: T): Int {
    var i = 0;
    for x in xs {
        if x == item {
            return i;
        }
        i += 1;
    }
    return -1;
}

fn count[T](xs: Array[T], item: T): Int {
    var n = 0;
    for x in xs {
        if x == item {
            n += 1;
        }
    }
    return n;
}

fn sum(xs: Array[Int]): Int {
    var n = 0;
    for x in xs {
        n += x;
    }
    return n;
}

fn min(xs: Array[Int], fallback: Int): Int {
    var found = false;
    var best = fallback;
    for x in xs {
        if found == false || x < best {
            best = x;
            found = true;
        }
    }
    return best;
}

fn max(xs: Array[Int], fallback: Int): Int {
    var found = false;
    var best = fallback;
    for x in xs {
        if found == false || x > best {
            best = x;
            found = true;
        }
    }
    return best;
}

fn join(xs: Array[String], sep: String): String {
    var out = "";
    var first = true;
    for x in xs {
        if first {
            out = x;
            first = false;
        } else {
            out = out + sep + x;
        }
    }
    return out;
}

fn reversed[T](xs: Array[T]): Array[T] {
    var out: Array[T] = [];
    var i = len(xs) - 1;
    while i >= 0 {
        out.push(xs[i]);
        i -= 1;
    }
    return out;
}
