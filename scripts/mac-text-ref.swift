// mac-text-ref.swift — CoreText 참조 렌더(T-100 검증 · 09-17): 파인더와 같은 경로(CoreText · Apple 스무딩 · 서브픽셀 위치)로
// 문자열을 1x 회색 비트맵에 그려 PGM(P5)으로 저장한다. 우리 앱(nexa-gfx CoreText 경로)의 같은 문자열 렌더와 픽셀 비교.
// 사용: swift scripts/mac-text-ref.swift <out.pgm> [family=Apple SD Gothic Neo] [px=15] [text]
import Foundation
import CoreText
import CoreGraphics

let args = CommandLine.arguments
let out = args.count > 1 ? args[1] : "target/textref/ref.pgm"
let family = args.count > 2 ? args[2] : "Apple SD Gothic Neo"
let px = args.count > 3 ? Double(args[3]) ?? 15 : 15
let text = args.count > 4 ? args[4] : "Nexa SQL Script_1.sql 한글 결과 0123 Hg"
let font = CTFontCreateWithName(family as CFString, px, nil)
let attrs: [NSAttributedString.Key: Any] = [kCTFontAttributeName as NSAttributedString.Key: font, kCTForegroundColorFromContextAttributeName as NSAttributedString.Key: true]
let line = CTLineCreateWithAttributedString(NSAttributedString(string: text, attributes: attrs))
let bounds = CTLineGetBoundsWithOptions(line, [])
let w = Int(ceil(bounds.width)) + 8, h = Int(ceil(CTFontGetAscent(font) + CTFontGetDescent(font))) + 8
var bits = [UInt8](repeating: 0, count: w * h)
let cs = CGColorSpaceCreateDeviceGray()
bits.withUnsafeMutableBytes { p in
    let ctx = CGContext(data: p.baseAddress, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w, space: cs, bitmapInfo: CGImageAlphaInfo.none.rawValue)!
    ctx.setFillColor(gray: 1, alpha: 1)
    ctx.setShouldAntialias(true)
    ctx.setAllowsFontSmoothing(true); ctx.setShouldSmoothFonts(true)
    ctx.setAllowsFontSubpixelPositioning(true); ctx.setShouldSubpixelPositionFonts(true)
    ctx.setAllowsFontSubpixelQuantization(false); ctx.setShouldSubpixelQuantizeFonts(false)
    ctx.textMatrix = .identity
    ctx.textPosition = CGPoint(x: 4, y: 4 + CTFontGetDescent(font))
    CTLineDraw(line, ctx)
}
// PGM은 위→아래 · CG는 아래→위.
var data = Data("P5\n\(w) \(h)\n255\n".utf8)
for y in (0..<h).reversed() { data.append(contentsOf: bits[y*w..<(y+1)*w]) }
try! data.write(to: URL(fileURLWithPath: out))
print("ref \(w)x\(h) -> \(out)")
