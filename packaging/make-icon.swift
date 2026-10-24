// Draws the app icon (1024 px PNG): a blue rounded square with a white circular arrow.
import AppKit
let size = 1024.0
let image = NSImage(size: NSSize(width: size, height: size))
image.lockFocus()
let rect = NSRect(x: 100, y: 100, width: 824, height: 824)
let tile = NSBezierPath(roundedRect: rect, xRadius: 185, yRadius: 185)
NSGradient(starting: NSColor(calibratedRed: 0.20, green: 0.42, blue: 0.95, alpha: 1), ending: NSColor(calibratedRed: 0.11, green: 0.25, blue: 0.70, alpha: 1))!.draw(in: tile, angle: -90)
let arc = NSBezierPath()
arc.appendArc(withCenter: NSPoint(x: 512, y: 512), radius: 250, startAngle: 120, endAngle: 420)
arc.lineWidth = 70
arc.lineCapStyle = .round
NSColor.white.setStroke()
arc.stroke()
let head = NSBezierPath()
let tip = NSPoint(x: 512 + 250 * cos(60 * .pi / 180), y: 512 + 250 * sin(60 * .pi / 180))
head.move(to: NSPoint(x: tip.x - 120, y: tip.y + 60))
head.line(to: NSPoint(x: tip.x + 40, y: tip.y + 40))
head.line(to: NSPoint(x: tip.x - 10, y: tip.y - 120))
head.close()
NSColor.white.setFill()
head.fill()
image.unlockFocus()
let rep = NSBitmapImageRep(data: image.tiffRepresentation!)!
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: CommandLine.arguments[1]))
