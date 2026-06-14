// Renders the installer SVG art to 24-bit BMP files that NSIS/MUI2 can use.
// NSIS requires uncompressed 24-bit BMP (no alpha), bottom-up rows padded to
// a 4-byte boundary. We rasterize with resvg, composite over the dark brand
// background, and hand-write the BMP container.
//
// Usage: node src-tauri/installer/build-assets.mjs
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { Resvg } from "@resvg/resvg-js";

const here = dirname(fileURLToPath(import.meta.url));

// Brand background; anti-aliased edges are composited onto this so the BMP can
// drop the alpha channel without fringing.
const BG = { r: 0x14, g: 0x16, b: 0x1b };

function renderRgba(svgPath, width) {
  const svg = readFileSync(svgPath);
  const resvg = new Resvg(svg, {
    fitTo: { mode: "width", value: width },
    font: { loadSystemFonts: true },
  });
  const img = resvg.render();
  return { pixels: img.pixels, width: img.width, height: img.height };
}

function rgbaToBmp24({ pixels, width, height }) {
  const rowBytes = width * 3;
  const padding = (4 - (rowBytes % 4)) % 4;
  const paddedRow = rowBytes + padding;
  const pixelDataSize = paddedRow * height;
  const fileSize = 54 + pixelDataSize;

  const buf = Buffer.alloc(fileSize);
  // BITMAPFILEHEADER
  buf.write("BM", 0, "ascii");
  buf.writeUInt32LE(fileSize, 2);
  buf.writeUInt32LE(0, 6);
  buf.writeUInt32LE(54, 10);
  // BITMAPINFOHEADER
  buf.writeUInt32LE(40, 14);
  buf.writeInt32LE(width, 18);
  buf.writeInt32LE(height, 22); // positive => bottom-up
  buf.writeUInt16LE(1, 26);
  buf.writeUInt16LE(24, 28);
  buf.writeUInt32LE(0, 30);
  buf.writeUInt32LE(pixelDataSize, 34);
  buf.writeInt32LE(2835, 38); // ~72 DPI
  buf.writeInt32LE(2835, 42);
  buf.writeUInt32LE(0, 46);
  buf.writeUInt32LE(0, 50);

  let offset = 54;
  for (let y = height - 1; y >= 0; y -= 1) {
    for (let x = 0; x < width; x += 1) {
      const i = (y * width + x) * 4;
      const a = pixels[i + 3] / 255;
      const r = Math.round(pixels[i] * a + BG.r * (1 - a));
      const g = Math.round(pixels[i + 1] * a + BG.g * (1 - a));
      const b = Math.round(pixels[i + 2] * a + BG.b * (1 - a));
      buf[offset++] = b;
      buf[offset++] = g;
      buf[offset++] = r;
    }
    offset += padding;
  }
  return buf;
}

function build(name, svgFile, width) {
  const rgba = renderRgba(join(here, svgFile), width);
  const bmp = rgbaToBmp24(rgba);
  const out = join(here, `${name}.bmp`);
  writeFileSync(out, bmp);
  console.log(`${name}.bmp  ${rgba.width}x${rgba.height}  ${bmp.length} bytes`);
}

build("sidebar", "sidebar.svg", 164);
build("header", "header.svg", 150);
