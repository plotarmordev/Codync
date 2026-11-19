package com.codync.android

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import coil3.ImageLoader
import com.codync.android.core.MarkdownBlocks
import com.codync.android.core.MarkdownBlocks.Block
import com.codync.android.design.*
import com.kyant.shapes.RoundedRectangle
import org.commonmark.ext.gfm.strikethrough.Strikethrough
import org.commonmark.ext.gfm.strikethrough.StrikethroughExtension
import org.commonmark.node.Code
import org.commonmark.node.Emphasis
import org.commonmark.node.HardLineBreak
import org.commonmark.node.HtmlInline
import org.commonmark.node.Image
import org.commonmark.node.Link
import org.commonmark.node.Node
import org.commonmark.node.SoftLineBreak
import org.commonmark.node.StrongEmphasis
import org.commonmark.parser.Parser

val LocalNetworkImages = staticCompositionLocalOf<ImageLoader> { error("Missing image loader") }

/**
 * The reply renderer of iOS `MarkdownText`: `MarkdownBlocks` for the blocks, inline styling for
 * bold, italic, code, strikethrough and links. Images show as their text: a reply never makes
 * the phone fetch an address.
 */
@Composable fun MarkdownText(source: String, selectable: Boolean = true, color: Color = Palette.text) {
    val blocks = remember(source) { MarkdownBlocks.parse(source) }
    val content: @Composable () -> Unit = {
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) { for (block in blocks) MarkdownBlock(block, color) }
    }
    if (selectable) SelectionContainer(content = content) else content()
}

/** One block of a reply (or of a shared Markdown file, in `FileViewer`). */
@Composable internal fun MarkdownBlock(block: Block, ink: Color) {
    val code = Palette.text.copy(alpha = 0.08f)
    when (block) {
        is Block.Paragraph -> Text(inline(block.text, code), style = Type.body, color = ink)
        is Block.Heading -> Text(inline(block.text, code), style = when (block.level) {
            1 -> Type.title3.bold; 2 -> Type.headline; else -> Type.subheadline.bold
        }, color = ink)
        is Block.Bullet -> Row(Modifier.padding(start = 16.dp * block.depth), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(block.marker, style = Type.body.monospacedDigit, color = Palette.secondary)
            Text(inline(block.text, code), style = Type.body, color = ink)
        }
        Block.Rule -> Box(Modifier.padding(vertical = 4.dp).fillMaxWidth().height(1.dp).background(Palette.border))
        is Block.Table -> MarkdownTable(block, code, ink)
        is Block.Quote -> Row(Modifier.height(IntrinsicSize.Min)) {
            Box(Modifier.width(3.dp).fillMaxHeight().background(Palette.border))
            Text(inline(block.text, code), Modifier.padding(start = 10.dp), style = Type.body, color = Palette.secondary)
        }
        is Block.Code -> Box(Modifier.fillMaxWidth().clip(RoundedRectangle(8.dp)).background(Palette.codeBackground)
            .horizontalScroll(rememberScrollState())) {
            Text(block.text, Modifier.padding(10.dp), style = Type.footnote.monospaced, color = Palette.text, softWrap = false)
        }
    }
}

/** A table: a bold header and hairlines between rows, its cells wrapping to fit the bubble; a
 *  table with too many columns to fit scrolls sideways. */
@Composable private fun MarkdownTable(table: Block.Table, code: Color, ink: Color) {
    val columns = maxOf(table.header.size, table.rows.maxOfOrNull { it.size } ?: 0)
    val rows = listOf(table.header) + table.rows
    BoxWithConstraints(Modifier.padding(vertical = 2.dp)) {
        val viewport = maxWidth
        // The scroll offers no width of its own, so the grid is told the bubble's.
        Box(Modifier.horizontalScroll(rememberScrollState())) {
            TableGrid(columns = columns, viewport = viewport) {
                for ((index, row) in rows.withIndex()) {
                    if (index > 0) Box(Modifier.fillMaxWidth().height(1.dp).background(Palette.border))
                    for (column in 0 until columns) {
                        Text(inline(row.getOrElse(column) { "" }, code), Modifier.heightIn(min = 22.dp),
                            style = if (index == 0) Type.body.semibold else Type.body, color = ink)
                    }
                }
            }
        }
    }
}

/** A table's cells in a grid that shares each row's height across its columns. A column is as
 *  wide as its widest cell when the columns fit in [viewport] together and, when they don't, the
 *  space is shared out equally, a column stopping at its widest cell (the desktop's `auto` grid
 *  tracks, iOS `TableLayout`). A cell wider than its column wraps mid-word. The children come row
 *  by row: each row after the first preceded by its hairline. */
@Composable private fun TableGrid(columns: Int, viewport: Dp, content: @Composable () -> Unit) {
    Layout(content) { measurables, constraints ->
        val gap = 16.dp.roundToPx()
        val rowGap = 7.dp.roundToPx()
        val available = (viewport.roundToPx() - gap * (columns - 1)).coerceAtLeast(0)
        val height = constraints.maxHeight

        // The children come row by row: the first `columns` cells, then (hairline, `columns` cells) each.
        val rows = buildList {
            add(measurables.take(columns))
            var at = columns
            while (at < measurables.size) {
                add(measurables.slice(at..<(at + 1 + columns)))
                at += 1 + columns
            }
        }
        val hairlines = rows.map { if (it.size == 1 + columns) it.first() else null }
        val cellRows = rows.map { if (it.size == 1 + columns) it.drop(1) else it }

        val maxContent = IntArray(columns) { column -> cellRows.maxOf { it[column].maxIntrinsicWidth(height) } }
        val widths = tableColumnWidths(maxContent, available)
        val tableWidth = widths.sum() + gap * (columns - 1)

        val placeableRows = cellRows.mapIndexed { index, row ->
            val hairline = hairlines[index]?.measure(Constraints(minWidth = tableWidth, maxWidth = tableWidth))
            val placeables = row.mapIndexed { column, cell -> cell.measure(Constraints(minWidth = widths[column], maxWidth = widths[column])) }
            hairline to placeables
        }
        // A row's height: its hairline with the gap under it, then the tallest cell.
        val rowHeights = placeableRows.map { (hairline, rowCells) -> (hairline?.height?.plus(rowGap) ?: 0) + rowCells.maxOf { it.height } }
        layout(tableWidth, (rowHeights.sum() + rowGap * (rowHeights.size - 1)).coerceAtLeast(constraints.minHeight)) {
            var y = 0
            placeableRows.forEach { (hairline, rowCells) ->
                hairline?.placeRelative(0, y)
                if (hairline != null) y += hairline.height + rowGap
                var x = 0
                rowCells.forEachIndexed { column, cell ->
                    cell.placeRelative(x, y)
                    x += widths[column] + gap
                }
                y += rowCells.maxOf { it.height } + rowGap
            }
        }
    }
}

/** A table's column widths: each as wide as its widest cell when they fit in [available]
 *  together, otherwise the space shared out equally, a column stopping at its widest cell. */
internal fun tableColumnWidths(maxContent: IntArray, available: Int): IntArray {
    if (maxContent.sum() <= available) return maxContent
    val widths = DoubleArray(maxContent.size)
    var remaining = available.toDouble()
    var growable = maxContent.indices.filter { maxContent[it] > 0 }
    while (remaining > 0 && growable.isNotEmpty()) {
        val each = remaining / growable.size
        var used = 0.0
        for (column in growable) {
            val add = minOf(each, maxContent[column] - widths[column])
            widths[column] += add
            used += add
        }
        remaining -= used
        growable = growable.filter { widths[it] < maxContent[it] }
    }
    return IntArray(maxContent.size) { widths[it].toInt() }
}

private val inlineParser = Parser.builder().extensions(listOf(StrikethroughExtension.create())).build()

/** Inline Markdown: links underlined (the theme is monochrome), inline code on a soft tint. */
internal fun inline(source: String, codeTint: Color): AnnotatedString {
    // Inline only: indentation would otherwise read as a code block.
    val document = inlineParser.parse(source.lines().joinToString("\n") { it.trimStart() })
    return buildAnnotatedString {
        fun visit(node: Node) {
            val style = when (node) {
                is StrongEmphasis -> SpanStyle(fontWeight = FontWeight.SemiBold)
                is Emphasis -> SpanStyle(fontStyle = FontStyle.Italic)
                is Code -> SpanStyle(fontFamily = FontFamily.Monospace, background = codeTint)
                is Strikethrough -> SpanStyle(textDecoration = TextDecoration.LineThrough)
                else -> null
            }
            if (style != null) pushStyle(style)
            val link = (node as? Link)?.destination?.takeIf { it.startsWith("https://") || it.startsWith("http://") || it.startsWith("mailto:") }
            if (link != null) pushLink(LinkAnnotation.Url(link, TextLinkStyles(SpanStyle(textDecoration = TextDecoration.Underline))))
            when (node) {
                is org.commonmark.node.Text -> append(node.literal)
                is Code -> append(node.literal)
                is SoftLineBreak, is HardLineBreak -> append("\n")
                is HtmlInline -> append(node.literal)
                is org.commonmark.node.HtmlBlock -> append(node.literal.trimEnd())
                is org.commonmark.node.FencedCodeBlock -> append(node.literal.trimEnd())
                is Image -> children(node).forEach(::visit)
                else -> children(node).forEach(::visit)
            }
            if (link != null) pop()
            if (style != null) pop()
        }
        // Paragraphs of a multi-line block keep their line breaks.
        children(document).forEachIndexed { index, paragraph -> if (index > 0) append("\n\n"); visit(paragraph) }
    }
}

private fun children(node: Node): List<Node> = buildList {
    var child = node.firstChild
    while (child != null) { add(child); child = child.next }
}

/** A monospaced box for commands and output (iOS `CodeBox`). */
@Composable fun CodeBox(text: String, style: TextStyle = Type.caption.monospaced) {
    Box(Modifier.fillMaxWidth().heightIn(max = 240.dp).clip(RoundedRectangle(8.dp)).background(Palette.codeBackground)
        .horizontalScroll(rememberScrollState())) {
        SelectionContainer { Text(text, Modifier.padding(8.dp), style = style, color = Palette.text, softWrap = false) }
    }
}
