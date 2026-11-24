pragma Singleton
import QtQuick

// Grove Preview's Markup geometry: outlines of the shapes that are drawn
// as polygons (star, polygon, speech bubble), and bounds, hit tests,
// moving and resizing of any op. Ops are in the page's or picture's own
// units (points for PDF pages, pixels for pictures).
QtObject {
    function box(o) {
        return [Math.min(o.x1, o.x2), Math.min(o.y1, o.y2), Math.max(o.x1, o.x2), Math.max(o.y1, o.y2)]
    }

    // The closed outline of a star, hexagon or speech bubble in its box.
    function outline(o) {
        const b = box(o)
        const cx = (b[0] + b[2]) / 2, cy = (b[1] + b[3]) / 2, rx = (b[2] - b[0]) / 2, ry = (b[3] - b[1]) / 2
        const p = []
        if (o.t === "star") {
            const n = o.n || 5
            for (let i = 0; i < n * 2; i++) {
                const a = -Math.PI / 2 + i * Math.PI / n
                const r = i % 2 ? 0.45 : 1
                p.push(cx + Math.cos(a) * rx * r, cy + Math.sin(a) * ry * r)
            }
        } else if (o.t === "poly") {
            const n = o.n || 6
            for (let i = 0; i < n; i++) {
                const a = -Math.PI / 2 + i * 2 * Math.PI / n
                p.push(cx + Math.cos(a) * rx, cy + Math.sin(a) * ry)
            }
        } else if (o.t === "bubble") {
            // A rounded box with a tail at the lower left.
            const h = (b[3] - b[1]) * 0.78, r = Math.min(b[2] - b[0], h) * 0.2
            const x0 = b[0], y0 = b[1], x1 = b[2], y1 = b[1] + h
            const arc = (ax, ay, a0) => { for (let i = 0; i <= 4; i++) { const a = a0 + i * Math.PI / 8; p.push(ax + Math.cos(a) * r, ay + Math.sin(a) * r) } }
            arc(x1 - r, y0 + r, -Math.PI / 2)
            arc(x1 - r, y1 - r, 0)
            p.push(x0 + (x1 - x0) * 0.42, y1, x0 + (x1 - x0) * 0.18, b[3], x0 + (x1 - x0) * 0.24, y1)
            arc(x0 + r, y1 - r, Math.PI / 2)
            arc(x0 + r, y0 + r, Math.PI)
        }
        return p
    }

    function withOutline(o) {
        if (o.t === "star" || o.t === "poly" || o.t === "bubble") o.p = outline(o)
        return o
    }

    // [x0, y0, x1, y1]
    function bounds(o) {
        if (o.t === "pen") {
            let x0 = 1e9, y0 = 1e9, x1 = -1e9, y1 = -1e9
            for (let i = 0; i + 1 < o.p.length; i += 2) { x0 = Math.min(x0, o.p[i]); y0 = Math.min(y0, o.p[i + 1]); x1 = Math.max(x1, o.p[i]); y1 = Math.max(y1, o.p[i + 1]) }
            const m = (o.w || 2) / 2
            return [x0 - m, y0 - m, x1 + m, y1 + m]
        }
        if (o.t === "hl" || o.t === "ul" || o.t === "so" || o.t === "sq") {
            let x0 = 1e9, y0 = 1e9, x1 = -1e9, y1 = -1e9
            for (const q of o.quads) for (let i = 0; i < 8; i += 2) { x0 = Math.min(x0, q[i]); y0 = Math.min(y0, q[i + 1]); x1 = Math.max(x1, q[i]); y1 = Math.max(y1, q[i + 1]) }
            return [x0, y0, x1, y1]
        }
        if (o.t === "note") return [o.x1, o.y1, o.x1 + 20, o.y1 + 20]
        return box(o)
    }

    function hit(o, x, y, tol) {
        const b = bounds(o)
        if (x < b[0] - tol || x > b[2] + tol || y < b[1] - tol || y > b[3] + tol) return false
        if (o.t === "pen") {
            for (let i = 0; i + 3 < o.p.length; i += 2)
                if (segDist(x, y, o.p[i], o.p[i + 1], o.p[i + 2], o.p[i + 3]) < tol + (o.w || 2) / 2) return true
            return o.p.length === 2 && Math.hypot(x - o.p[0], y - o.p[1]) < tol + (o.w || 2)
        }
        if (o.t === "line" || o.t === "arrow") return segDist(x, y, o.x1, o.y1, o.x2, o.y2) < tol + (o.w || 2)
        if ((o.t === "rect" || o.t === "ellipse" || o.t === "rrect") && !o.fill) {
            // Outline only: near the edge.
            const inX = x > b[0] + tol && x < b[2] - tol, inY = y > b[1] + tol && y < b[3] - tol
            return !(inX && inY)
        }
        return true
    }

    function segDist(px, py, ax, ay, bx, by) {
        const dx = bx - ax, dy = by - ay
        const l = dx * dx + dy * dy
        let t = l > 0 ? ((px - ax) * dx + (py - ay) * dy) / l : 0
        t = Math.max(0, Math.min(1, t))
        return Math.hypot(px - (ax + t * dx), py - (ay + t * dy))
    }

    function moved(o, dx, dy) {
        const n = JSON.parse(JSON.stringify(o))
        if (n.p) for (let i = 0; i + 1 < n.p.length; i += 2) { n.p[i] += dx; n.p[i + 1] += dy }
        if (n.quads) n.quads = n.quads.map(q => q.map((v, i) => v + (i % 2 ? dy : dx)))
        if (n.x1 !== undefined) { n.x1 += dx; n.y1 += dy }
        if (n.x2 !== undefined) { n.x2 += dx; n.y2 += dy }
        return n
    }

    // The op stretched from its bounds to `nb` ([x0, y0, x1, y1]).
    function resized(o, nb) {
        const b = bounds(o)
        const sx = (nb[2] - nb[0]) / Math.max(0.01, b[2] - b[0]), sy = (nb[3] - nb[1]) / Math.max(0.01, b[3] - b[1])
        const fx = v => nb[0] + (v - b[0]) * sx, fy = v => nb[1] + (v - b[1]) * sy
        const n = JSON.parse(JSON.stringify(o))
        if (n.t === "pen") for (let i = 0; i + 1 < n.p.length; i += 2) { n.p[i] = fx(n.p[i]); n.p[i + 1] = fy(n.p[i + 1]) }
        else if (n.quads) n.quads = n.quads.map(q => q.map((v, i) => i % 2 ? fy(v) : fx(v)))
        else if (n.x2 !== undefined) {
            n.x1 = fx(n.x1); n.y1 = fy(n.y1); n.x2 = fx(n.x2); n.y2 = fy(n.y2)
            withOutline(n)
        }
        return n
    }
}
