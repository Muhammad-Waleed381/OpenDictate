use tauri::image::Image;

const SS: f64 = 4.0;
const SIZE: u32 = 22;

struct Canvas {
    w: u32,
    h: u32,
    buf: Vec<[f64; 4]>,
}

impl Canvas {
    fn new(w: u32, h: u32) -> Self {
        Self {
            w,
            h,
            buf: vec![[0.0; 4]; (w * h) as usize],
        }
    }

    fn blend(&mut self, x: u32, y: u32, c: [f64; 4]) {
        if x >= self.w || y >= self.h {
            return;
        }
        let i = (y * self.w + x) as usize;
        let d = &mut self.buf[i];
        let sa = c[3];
        let da = d[3];
        let out_a = sa + da * (1.0 - sa);
        if out_a > 0.0 {
            for k in 0..3 {
                d[k] = (c[k] * sa + d[k] * da * (1.0 - sa)) / out_a;
            }
            d[3] = out_a;
        }
    }

    fn circle(&mut self, cx: f64, cy: f64, r: f64, c: [f64; 4]) {
        let x0 = (cx - r).floor().max(0.0) as u32;
        let x1 = (cx + r).ceil().min(self.w as f64) as u32;
        let y0 = (cy - r).floor().max(0.0) as u32;
        let y1 = (cy + r).ceil().min(self.h as f64) as u32;
        for y in y0..y1 {
            for x in x0..x1 {
                let dx = x as f64 + 0.5 - cx;
                let dy = y as f64 + 0.5 - cy;
                if dx * dx + dy * dy <= r * r {
                    self.blend(x, y, c);
                }
            }
        }
    }

    fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, c: [f64; 4]) {
        let x0 = x.floor().max(0.0) as u32;
        let x1 = (x + w).ceil().min(self.w as f64) as u32;
        let y0 = y.floor().max(0.0) as u32;
        let y1 = (y + h).ceil().min(self.h as f64) as u32;
        for yy in y0..y1 {
            for xx in x0..x1 {
                self.blend(xx, yy, c);
            }
        }
    }

    fn round_rect(&mut self, x: f64, y: f64, w: f64, h: f64, r: f64, c: [f64; 4]) {
        self.rect(x + r, y, w - 2.0 * r, h, c);
        self.rect(x, y + r, w, h - 2.0 * r, c);
        self.circle(x + r, y + r, r, c);
        self.circle(x + w - r, y + r, r, c);
        self.circle(x + r, y + h - r, r, c);
        self.circle(x + w - r, y + h - r, r, c);
    }

    fn line(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, w: f64, c: [f64; 4]) {
        let steps = ((x1 - x0).hypot(y1 - y0) * SS * 2.0).ceil() as u32;
        let t = w / 2.0;
        for s in 0..=steps {
            let f = s as f64 / steps as f64;
            self.circle(x0 + (x1 - x0) * f, y0 + (y1 - y0) * f, t, c);
        }
    }

    fn paint(&self) -> Vec<u8> {
        let mut out = vec![0u8; (SIZE * SIZE * 4) as usize];
        let scale = self.w as f64 / SIZE as f64;
        for y in 0..SIZE {
            for x in 0..SIZE {
                let mut acc = [0.0; 4];
                let sx0 = (x as f64 * scale).floor() as u32;
                let sx1 = ((x as f64 + 1.0) * scale).ceil().min(self.w as f64) as u32;
                let sy0 = (y as f64 * scale).floor() as u32;
                let sy1 = ((y as f64 + 1.0) * scale).ceil().min(self.h as f64) as u32;
                let mut n = 0.0;
                for yy in sy0..sy1 {
                    for xx in sx0..sx1 {
                        let p = self.buf[(yy * self.w + xx) as usize];
                        if p[3] > 0.0 {
                            for k in 0..4 {
                                acc[k] += p[k] * p[3];
                            }
                            n += p[3];
                        }
                    }
                }
                if n > 0.0 {
                    acc[0] /= n;
                    acc[1] /= n;
                    acc[2] /= n;
                    acc[3] /= n;
                    let i = ((y * SIZE + x) * 4) as usize;
                    out[i] = (acc[0] * 255.0) as u8;
                    out[i + 1] = (acc[1] * 255.0) as u8;
                    out[i + 2] = (acc[2] * 255.0) as u8;
                    out[i + 3] = (acc[3] * 255.0) as u8;
                }
            }
        }
        out
    }
}

fn mic(canvas: &mut Canvas, c: [f64; 4]) {
    canvas.circle(11.0 * SS, 6.0 * SS, 3.7 * SS, c);
    canvas.round_rect(8.0 * SS, 8.5 * SS, 6.0 * SS, 4.8 * SS, 2.6 * SS, c);
    canvas.rect(10.3 * SS, 13.0 * SS, 1.4 * SS, 2.9 * SS, c);
    canvas.round_rect(8.0 * SS, 15.6 * SS, 6.0 * SS, 1.5 * SS, 0.75 * SS, c);
}

fn check(canvas: &mut Canvas, c: [f64; 4]) {
    canvas.line(6.5 * SS, 11.5 * SS, 9.8 * SS, 14.8 * SS, 2.2 * SS, c);
    canvas.line(9.8 * SS, 14.8 * SS, 15.5 * SS, 7.5 * SS, 2.2 * SS, c);
}

fn cross(canvas: &mut Canvas, c: [f64; 4]) {
    canvas.line(7.0 * SS, 7.0 * SS, 15.0 * SS, 15.0 * SS, 2.4 * SS, c);
    canvas.line(15.0 * SS, 7.0 * SS, 7.0 * SS, 15.0 * SS, 2.4 * SS, c);
}

fn glyph(status: &str) -> Vec<u8> {
    let mut canvas = Canvas::new(22 * 4, 22 * 4);
    let backdrop = [0.0, 0.0, 0.0, 0.32];
    canvas.circle(11.0 * SS, 11.0 * SS, 11.0 * SS, backdrop);

    match status {
        "listening" => mic(&mut canvas, [1.0, 0.30, 0.30, 1.0]),
        "transcribing" => mic(&mut canvas, [1.0, 0.69, 0.13, 1.0]),
        "inserted" => check(&mut canvas, [0.20, 0.83, 0.60, 1.0]),
        "error" => cross(&mut canvas, [1.0, 0.32, 0.32, 1.0]),
        _ => mic(&mut canvas, [1.0, 1.0, 1.0, 1.0]),
    }

    canvas.paint()
}

pub fn icon_for_status(status: &str) -> Image<'static> {
    Image::new_owned(glyph(status), SIZE, SIZE)
}