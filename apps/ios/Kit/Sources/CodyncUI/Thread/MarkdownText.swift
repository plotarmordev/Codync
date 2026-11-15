import CodyncKit
import SwiftUI

/// The iPhone's Markdown renderer for messages (`MarkdownBlocks`: tables, rules, nested lists
/// too); inline styling comes from `AttributedString(markdown:)`.
/// Equatable so a reply that didn't change skips its body.
public struct MarkdownText: View, Equatable {
    let source: String

    public init(_ source: String) {
        self.source = source
    }

    public var body: some View {
        let blocks = MarkdownBlocks.parse(source)
        VStack(alignment: .leading, spacing: 8) {
            ForEach(Array(blocks.enumerated()), id: \.offset) { _, block in
                MarkdownBlock(block: block)
            }
        }
        .textSelection(.enabled)
    }

    /// Underlines keep links distinct in the monochrome theme; inline code gets a soft tint.
    static func inline(_ s: String) -> AttributedString {
        guard var text = try? AttributedString(markdown: s, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace))
        else { return AttributedString(s) }
        for run in text.runs where run.link != nil {
            text[run.range].underlineStyle = .single
        }
        for run in text.runs where run.inlinePresentationIntent?.contains(.code) == true {
            text[run.range].backgroundColor = Palette.text.opacity(0.08)
        }
        return text
    }
}

/// One block of a reply (or of a shared Markdown file, in `FileViewer`).
struct MarkdownBlock: View {
    let block: MarkdownBlocks.Block

    @ViewBuilder var body: some View {
        switch block {
        case let .paragraph(t):
            Text(MarkdownText.inline(t))
                .font(.body)
                .foregroundStyle(Palette.text)
        case let .heading(t, level):
            Text(MarkdownText.inline(t))
                .font(level == 1 ? .title3.bold() : level == 2 ? .headline : .subheadline.bold())
                .foregroundStyle(Palette.text)
        case let .bullet(t, marker, depth):
            HStack(alignment: .firstTextBaseline, spacing: 6) {
                Text(marker).foregroundStyle(Palette.secondary).monospacedDigit()
                Text(MarkdownText.inline(t)).foregroundStyle(Palette.text)
            }
            .font(.body)
            .padding(.leading, CGFloat(depth) * 16)
        case .rule:
            Rectangle().fill(Palette.border).frame(height: 1).padding(.vertical, 4)
        case let .table(header, rows):
            MarkdownTable(header: header, rows: rows)
        case let .quote(t):
            Text(MarkdownText.inline(t))
                .font(.body)
                .foregroundStyle(Palette.secondary)
                .padding(.leading, 10)
                .overlay(alignment: .leading) { Rectangle().fill(Palette.border).frame(width: 3) }
        case let .code(t, _):
            ScrollView(.horizontal, showsIndicators: false) {
                Text(t)
                    .font(.system(.footnote, design: .monospaced))
                    .foregroundStyle(Palette.text)
                    .padding(10)
            }
            .background(Palette.codeBackground, in: RoundedRectangle(cornerRadius: 8))
        }
    }
}

/// A table: a bold header and hairlines between rows, its cells wrapping to fit the bubble. A
/// word longer than the space its column gets wraps mid-word, since SwiftUI's Text reports no
/// minimum width to hold a whole word (the desktop and Android scroll such tables instead).
private struct MarkdownTable: View {
    let header: [String]
    let rows: [[String]]

    var body: some View {
        let columns = max(header.count, rows.map(\.count).max() ?? 0)
        TableLayout(columns: columns) { TableCells(header: header, rows: rows, columns: columns) }
            .font(.body)
            .foregroundStyle(Palette.text)
            .padding(.vertical, 2)
    }
}

/// The table's children: a row of header cells, then a hairline and a row of cells per body row.
private struct TableCells: View {
    let header: [String]
    let rows: [[String]]
    let columns: Int

    var body: some View {
        ForEach(Array(([header] + rows).enumerated()), id: \.offset) { index, row in
            if index > 0 { Rectangle().fill(Palette.border).frame(height: 1) }
            ForEach(0 ..< columns, id: \.self) { column in
                Text(MarkdownText.inline(row.indices.contains(column) ? row[column] : ""))
                    .fontWeight(index == 0 ? .semibold : .regular)
            }
        }
    }
}

/// A table's cells in a grid that shares each row's height across its columns. A column is as
/// wide as its widest cell when the columns fit together, and when they don't, the space is
/// shared out equally and a column stops growing at its widest cell — the desktop's `auto` grid
/// tracks. Proposed no width, every column takes its widest cell.
private struct TableLayout: Layout {
    let columns: Int

    struct Cache {
        var proposal: ProposedViewSize?
        var widths: [CGFloat] = []
        var heights: [[CGFloat]] = []
    }

    func makeCache(subviews: LayoutSubviews) -> Cache { Cache() }

    /// The cache only shortcuts re-measuring an unchanged proposal; new subviews (a message that
    /// streamed another row) measure anew.
    func updateCache(_ cache: inout Cache, subviews: LayoutSubviews) { cache.proposal = nil }
    func sizeThatFits(proposal: ProposedViewSize, subviews: LayoutSubviews, cache: inout Cache) -> CGSize {
        measure(proposal: proposal, subviews: subviews, cache: &cache)
        let height = cache.heights.map { $0.max() ?? 0 }.reduce(0, +) + 15 * CGFloat(max(0, cache.heights.count - 1))
        return CGSize(width: tableWidth(cache.widths), height: height)
    }

    /// The table's width: its columns and the gaps between them, which its hairlines span too.
    private func tableWidth(_ widths: [CGFloat]) -> CGFloat {
        widths.reduce(0, +) + CGFloat(columns - 1) * 16
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: LayoutSubviews, cache: inout Cache) {
        measure(proposal: proposal, subviews: subviews, cache: &cache)
        var y = bounds.minY
        var index = 0
        for (rowNumber, row) in cache.heights.enumerated() {
            if rowNumber > 0 {
                subviews[index].place(at: CGPoint(x: bounds.minX, y: y), proposal: ProposedViewSize(width: tableWidth(cache.widths), height: 1))
                y += 1 + 7
                index += 1
            }
            var x = bounds.minX
            for (column, cellHeight) in row.enumerated() {
                subviews[index].place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(width: cache.widths[column], height: cellHeight))
                x += cache.widths[column] + 16
                index += 1
            }
            y += (row.max() ?? 0) + 7
        }
    }

    private func measure(proposal: ProposedViewSize, subviews: LayoutSubviews, cache: inout Cache) {
        if cache.proposal == proposal { return }

        let rows = cellRows(contentCount: subviews.count)
        let ideal = (0 ..< columns).map { column in
            rows.map { subviews[$0[column]].sizeThatFits(.unspecified).width }.max() ?? 0
        }
        let available = proposal.width.map { $0 - CGFloat(columns - 1) * 16 }
        var widths = ideal
        if let available, ideal.reduce(0, +) > available {
            widths = Array(repeating: 0, count: columns)
            var remaining = available
            var growable = Array(0 ..< columns)
            while remaining > 0, !growable.isEmpty {
                let share = remaining / CGFloat(growable.count)
                var used: CGFloat = 0
                for column in growable {
                    let add = min(share, ideal[column] - widths[column])
                    widths[column] += add
                    used += add
                }
                remaining -= used
                growable = growable.filter { widths[$0] < ideal[$0] }
            }
        }
        cache.proposal = proposal
        cache.widths = widths
        cache.heights = rows.map { row in
            row.enumerated().map { column, cell in subviews[cell].sizeThatFits(ProposedViewSize(width: widths[column], height: nil)).height }
        }
    }

    /// The children that are cells, row by row: the first `columns`, then every `columns` after a
    /// hairline.
    private func cellRows(contentCount: Int) -> [[Int]] {
        var rows: [[Int]] = []
        var index = 0
        while index < contentCount {
            if !rows.isEmpty { index += 1 }  // the hairline before this row
            rows.append(Array(index ..< index + columns))
            index += columns
        }
        return rows
    }
}
