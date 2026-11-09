fn abs(n: Int): Int {
    if n < 0 {
        return -n;
    }
    return n;
}

fn sign(n: Int): Int {
    if n > 0 {
        return 1;
    }
    if n < 0 {
        return -1;
    }
    return 0;
}

fn min(a: Int, b: Int): Int {
    if a < b {
        return a;
    }
    return b;
}

fn max(a: Int, b: Int): Int {
    if a > b {
        return a;
    }
    return b;
}

fn clamp(v: Int, lo: Int, hi: Int): Int {
    if v < lo {
        return lo;
    }
    if v > hi {
        return hi;
    }
    return v;
}

fn gcd(a: Int, b: Int): Int {
    var x = a;
    var y = b;
    if x < 0 {
        x = -x;
    }
    if y < 0 {
        y = -y;
    }
    while y != 0 {
        var t = y;
        y = x % y;
        x = t;
    }
    return x;
}

fn pow(a: Int, b: Int): Int {
    return __math.pow(a, b);
}

fn rand_int(n: Int): Int {
    return __math.rand_int(n);
}

fn sin(n: Float): Float {
    return __math.sin(n);
}

fn cos(n: Float): Float {
    return __math.cos(n);
}

fn atan2(y: Float, x: Float): Float {
    return __math.atan2(y, x);
}

fn sqrt(n: Float): Float {
    return __math.sqrt(n);
}

fn powf(a: Float, b: Float): Float {
    return __math.powf(a, b);
}

fn to_int(n: Float): Int {
    return __math.to_int(n);
}

fn to_float(n: Int): Float {
    return __math.to_float(n);
}

fn floor(n: Float): Float {
    return __math.floor(n);
}

fn ceil(n: Float): Float {
    return __math.ceil(n);
}

fn random(): Float {
    return __math.random();
}

fn lerp(a: Float, b: Float, t: Float): Float {
    return a + (b - a) * t;
}

fn move_toward(current: Float, target: Float, delta: Float): Float {
    var step = delta;
    if step < 0 {
        step = -step;
    }
    var d = target - current;
    if d < 0 {
        d = -d;
    }
    if d <= step {
        return target;
    }
    if target > current {
        return current + step;
    }
    return current - step;
}
