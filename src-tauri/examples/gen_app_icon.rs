use image::{Rgba, RgbaImage};

const SS: u32 = 2;
const SIZE: u32 = 512;

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
        let r = r.min(w / 2.0).min(h / 2.0);
        self.rect(x + r, y, w - 2.0 * r, h, c);
        self.rect(x, y + r, w, h - 2.0 * r, c);
        self.circle(x + r, y + r, r, c);
        self.circle(x + w - r, y + r, r, c);
        self.circle(x + r, y + h - r, r, c);
        self.circle(x + w - r, y + h - r, r, c);
    }

    fn paint(&self) -> RgbaImage {
        let mut img = RgbaImage::new(SIZE, SIZE);
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
                    img.put_pixel(
                        x,
                        y,
                        Rgba([
                            (acc[0] * 255.0) as u8,
                            (acc[1] * 255.0) as u8,
                            (acc[2] * 255.0) as u8,
                            (acc[3] * 255.0) as u8,
                        ]),
                    );
                }
            }
        }
        img
    }
}

fn mic(canvas: &mut Canvas, s: f64, ox: f64, oy: f64, c: [f64; 4]) {
    canvas.circle(11.0 * s + ox, 6.0 * s + oy, 3.7 * s, c);
    canvas.round_rect(8.0 * s + ox, 8.5 * s + oy, 6.0 * s, 4.8 * s, 2.6 * s, c);
    canvas.rect(10.3 * s + ox, 13.0 * s + oy, 1.4 * s, 2.9 * s, c);
    canvas.round_rect(8.0 * s + ox, 15.6 * s + oy, 6.0 * s, 1.5 * s, 0.75 * s, c);
}

fn main() {
    let mut canvas = Canvas::new(SIZE * SS, SIZE * SS);
    let s = SS as f64;

    canvas.round_rect(
        0.0,
        0.0,
        SIZE as f64 * s,
        SIZE as f64 * s,
        115.0 * s,
        [0.09, 0.09, 0.11, 1.0],
    );
    canvas.round_rect(
        0.0,
        0.0,
        SIZE as f64 * s,
        SIZE as f64 * s,
        115.0 * s,
        [0.30, 0.30, 0.34, 0.25],
    );

    let m = 17.0 * s;
    let ox = 69.0 * s;
    let oy = 66.0 * s;
    let white = [1.0, 1.0, 1.0, 1.0];
    mic(&mut canvas, m, ox, oy, white);

    canvas.circle(444.0 * s, 456.0 * s, 40.0 * s, [0.90, 0.28, 0.30, 1.0]);

    canvas
        .paint()
        .save("/tmp/opencode/app_icon_512.png")
        .unwrap();
    println!("wrote /tmp/opencode/app_icon_512.png");
}