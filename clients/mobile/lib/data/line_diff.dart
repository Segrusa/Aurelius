/// One line of a line diff: kept, removed (only in the old text) or added.
enum DiffOp { same, removed, added }

typedef DiffLine = ({DiffOp op, String text});

/// The lines of [before] and [after] as a diff (longest common subsequence),
/// removed lines before the added ones that replace them. Instructions are
/// short, so the quadratic table is fine; above [maxCells] it falls back to
/// "all removed, all added".
List<DiffLine> lineDiff(String before, String after, {int maxCells = 4000000}) {
  final a = before.isEmpty ? const <String>[] : before.split('\n');
  final b = after.isEmpty ? const <String>[] : after.split('\n');
  if (a.length * b.length > maxCells) {
    return [for (final l in a) (op: DiffOp.removed, text: l), for (final l in b) (op: DiffOp.added, text: l)];
  }
  // lcs[i][j]: length of the common subsequence of a[i..] and b[j..].
  final lcs = List.generate(a.length + 1, (_) => List<int>.filled(b.length + 1, 0));
  for (var i = a.length - 1; i >= 0; i--) {
    for (var j = b.length - 1; j >= 0; j--) {
      lcs[i][j] = a[i] == b[j] ? lcs[i + 1][j + 1] + 1 : (lcs[i + 1][j] >= lcs[i][j + 1] ? lcs[i + 1][j] : lcs[i][j + 1]);
    }
  }
  final out = <DiffLine>[];
  var i = 0, j = 0;
  while (i < a.length && j < b.length) {
    if (a[i] == b[j]) {
      out.add((op: DiffOp.same, text: a[i]));
      i++;
      j++;
    } else if (lcs[i + 1][j] >= lcs[i][j + 1]) {
      out.add((op: DiffOp.removed, text: a[i++]));
    } else {
      out.add((op: DiffOp.added, text: b[j++]));
    }
  }
  while (i < a.length) {
    out.add((op: DiffOp.removed, text: a[i++]));
  }
  while (j < b.length) {
    out.add((op: DiffOp.added, text: b[j++]));
  }
  return out;
}
