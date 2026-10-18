// Desenha o ícone do Painel do Site (1024×1024) e grava em PNG.
// Uso: swift desenhar-icone.swift saida.png
//
// Folha branca com linhas de texto (escrever) e um selo verde com seta
// para cima (enviar ao site), sobre o quadrado arredondado azul do macOS.

import AppKit

let size: CGFloat = 1024
let out = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "icone.png"

func rgb(_ r: CGFloat, _ g: CGFloat, _ b: CGFloat, _ a: CGFloat = 1) -> CGColor {
    CGColor(red: r / 255, green: g / 255, blue: b / 255, alpha: a)
}

/// Retângulo em coordenadas "de cima para baixo" (y cresce para baixo).
func rect(_ x: CGFloat, _ y: CGFloat, _ w: CGFloat, _ h: CGFloat) -> CGRect {
    CGRect(x: x, y: size - y - h, width: w, height: h)
}

func pt(_ x: CGFloat, _ y: CGFloat) -> CGPoint { CGPoint(x: x, y: size - y) }

let rep = NSBitmapImageRep(
    bitmapDataPlanes: nil, pixelsWide: Int(size), pixelsHigh: Int(size), bitsPerSample: 8,
    samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
    bytesPerRow: 0, bitsPerPixel: 0)!
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
let ctx = NSGraphicsContext.current!.cgContext
let space = CGColorSpaceCreateDeviceRGB()

// --- fundo: quadrado arredondado com gradiente (grade de ícones do macOS) ---
let body = rect(100, 100, 824, 824)
let bodyPath = CGPath(roundedRect: body, cornerWidth: 186, cornerHeight: 186, transform: nil)

ctx.saveGState()
ctx.setShadow(offset: CGSize(width: 0, height: -10), blur: 28, color: rgb(0, 0, 0, 0.35))
ctx.addPath(bodyPath)
ctx.setFillColor(rgb(37, 99, 235))
ctx.fillPath()
ctx.restoreGState()

ctx.saveGState()
ctx.addPath(bodyPath)
ctx.clip()
let bg = CGGradient(colorsSpace: space, colors: [rgb(77, 141, 255), rgb(37, 99, 235), rgb(30, 64, 175)] as CFArray, locations: [0, 0.5, 1])!
ctx.drawLinearGradient(bg, start: pt(512, 100), end: pt(512, 924), options: [])
// brilho suave no alto
let shine = CGGradient(colorsSpace: space, colors: [rgb(255, 255, 255, 0.18), rgb(255, 255, 255, 0)] as CFArray, locations: [0, 1])!
ctx.drawRadialGradient(shine, startCenter: pt(380, 160), startRadius: 0, endCenter: pt(380, 160), endRadius: 620, options: [])
ctx.restoreGState()

// --- folha com canto dobrado ---
let (px, py, pw, ph, fold): (CGFloat, CGFloat, CGFloat, CGFloat, CGFloat) = (262, 214, 440, 590, 120)
let page = CGMutablePath()
page.move(to: pt(px + 34, py))
page.addLine(to: pt(px + pw - fold, py))
page.addLine(to: pt(px + pw, py + fold))
page.addLine(to: pt(px + pw, py + ph - 34))
page.addQuadCurve(to: pt(px + pw - 34, py + ph), control: pt(px + pw, py + ph))
page.addLine(to: pt(px + 34, py + ph))
page.addQuadCurve(to: pt(px, py + ph - 34), control: pt(px, py + ph))
page.addLine(to: pt(px, py + 34))
page.addQuadCurve(to: pt(px + 34, py), control: pt(px, py))
page.closeSubpath()

ctx.saveGState()
ctx.setShadow(offset: CGSize(width: 0, height: -14), blur: 34, color: rgb(10, 30, 90, 0.45))
ctx.addPath(page)
ctx.setFillColor(rgb(255, 255, 255))
ctx.fillPath()
ctx.restoreGState()

let corner = CGMutablePath()
corner.move(to: pt(px + pw - fold, py))
corner.addLine(to: pt(px + pw - fold, py + fold - 22))
corner.addQuadCurve(to: pt(px + pw - fold + 22, py + fold), control: pt(px + pw - fold, py + fold))
corner.addLine(to: pt(px + pw, py + fold))
corner.closeSubpath()
ctx.addPath(corner)
ctx.setFillColor(rgb(203, 220, 245))
ctx.fillPath()

// --- linhas de texto ---
func bar(_ x: CGFloat, _ y: CGFloat, _ w: CGFloat, _ h: CGFloat, _ c: CGColor) {
    ctx.addPath(CGPath(roundedRect: rect(x, y, w, h), cornerWidth: h / 2, cornerHeight: h / 2, transform: nil))
    ctx.setFillColor(c)
    ctx.fillPath()
}
let lx = px + 62
bar(lx, py + 96, 220, 40, rgb(37, 99, 235))            // título
for (i, w) in [316, 290, 316, 200].enumerated() {      // parágrafo
    bar(lx, py + 186 + CGFloat(i) * 62, CGFloat(w), 26, rgb(196, 207, 222))
}

// --- selo verde com seta para cima ---
let c = pt(690, 690)
let r: CGFloat = 150
ctx.saveGState()
ctx.setShadow(offset: CGSize(width: 0, height: -10), blur: 26, color: rgb(0, 40, 20, 0.40))
ctx.addEllipse(in: CGRect(x: c.x - r - 18, y: c.y - r - 18, width: 2 * (r + 18), height: 2 * (r + 18)))
ctx.setFillColor(rgb(255, 255, 255))
ctx.fillPath()
ctx.restoreGState()

ctx.saveGState()
ctx.addEllipse(in: CGRect(x: c.x - r, y: c.y - r, width: 2 * r, height: 2 * r))
ctx.clip()
let green = CGGradient(colorsSpace: space, colors: [rgb(52, 211, 120), rgb(22, 163, 74)] as CFArray, locations: [0, 1])!
ctx.drawLinearGradient(green, start: CGPoint(x: c.x, y: c.y + r), end: CGPoint(x: c.x, y: c.y - r), options: [])
ctx.restoreGState()

let arrow = CGMutablePath()
arrow.move(to: CGPoint(x: c.x, y: c.y + 92))          // ponta
arrow.addLine(to: CGPoint(x: c.x + 82, y: c.y + 6))
arrow.addLine(to: CGPoint(x: c.x + 30, y: c.y + 6))
arrow.addLine(to: CGPoint(x: c.x + 30, y: c.y - 84))
arrow.addLine(to: CGPoint(x: c.x - 30, y: c.y - 84))
arrow.addLine(to: CGPoint(x: c.x - 30, y: c.y + 6))
arrow.addLine(to: CGPoint(x: c.x - 82, y: c.y + 6))
arrow.closeSubpath()
ctx.addPath(arrow)
ctx.setLineJoin(.round)
ctx.setLineWidth(14)
ctx.setStrokeColor(rgb(255, 255, 255))
ctx.setFillColor(rgb(255, 255, 255))
ctx.drawPath(using: .fillStroke)

NSGraphicsContext.current = nil
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: out))
print("Ícone gravado em \(out)")
