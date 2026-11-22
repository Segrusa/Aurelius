// Design artifacts (ticket #79, phase C): an HTML file a run kept with its
// message, opened as a live design. Its versions are the kept HTML files with
// the same name in the same conversation. The same rules as the desktop's
// `artifact-logic.ts`, so both apps count the same v1…vN.

import 'package:intl/intl.dart';

import '../../data/protocol/models.dart';

/// A kept file the Artifact page opens; every other file keeps the File page.
bool isArtifact(FileRef file) => file.mediaType.split(';').first.trim().toLowerCase() == 'text/html';

/// One version of an artifact: the file, its number and the message that kept it.
class ArtifactVersion {
  const ArtifactVersion({required this.version, required this.file, required this.messageId, required this.atMs});

  /// 1 for the first message that kept this name, counting up in message order.
  final int version;
  final FileRef file;
  final String messageId;
  final int atMs;

  @override
  bool operator ==(Object other) =>
      other is ArtifactVersion && other.version == version && other.file.id == file.id && other.messageId == messageId && other.atMs == atMs;

  @override
  int get hashCode => Object.hash(version, file.id, messageId, atMs);
}

/// All versions of the artifact called [name], newest first. A deleted message
/// no longer offers its file. Two files with the same name in one message are
/// two versions, in the message's order.
List<ArtifactVersion> artifactVersions(List<MessageView> messages, String name) {
  final ordered = messages.where((m) => m.deletedAtMs == null).toList()..sort((a, b) => a.ordinal.compareTo(b.ordinal));
  final kept = [
    for (final m in ordered)
      for (final f in m.files)
        if (f.name == name && isArtifact(f)) (file: f, messageId: m.id, atMs: m.createdAtMs),
  ];
  return [
    for (var i = kept.length - 1; i >= 0; i--) ArtifactVersion(version: i + 1, file: kept[i].file, messageId: kept[i].messageId, atMs: kept[i].atMs),
  ];
}

/// The versions the page offers for a file that was opened: those of the
/// conversation, or the file alone when its message is not in the list (yet).
List<ArtifactVersion> versionsFor(List<MessageView> messages, FileRef file, int atMs) {
  final versions = artifactVersions(messages, file.name);
  return versions.any((v) => v.file.id == file.id) ? versions : [ArtifactVersion(version: 1, file: file, messageId: '', atMs: atMs)];
}

/// "v2 of 3" for a file's chip; null when there is only one.
String? versionOf(List<ArtifactVersion> versions, String fileId) {
  final own = versions.where((v) => v.file.id == fileId).firstOrNull;
  return own != null && versions.length > 1 ? 'v${own.version} of ${versions.length}' : null;
}

/// "v3 · 14:05", with the date when it was not today.
String versionLabel(ArtifactVersion v, DateTime now) {
  final at = DateTime.fromMillisecondsSinceEpoch(v.atMs);
  final time = DateFormat('HH:mm').format(at);
  final today = at.year == now.year && at.month == now.month && at.day == now.day;
  return 'v${v.version} · ${today ? time : '${DateFormat('MMM d').format(at)}, $time'}';
}

/// The widths the page shows an artifact at, as on the desktop.
enum ArtifactViewport {
  desktop('Desktop', 1280),
  tablet('Tablet', 768),
  mobile('Mobile', 390);

  const ArtifactViewport(this.label, this.width);

  final String label;

  /// CSS pixels: the page lays the artifact out this wide and scales it to fit.
  final int width;
}

/// The longest feedback a board member types on the phone; the message itself
/// has the server's own limit, this keeps the field sane.
const artifactFeedbackMax = 4000;

/// The most characters of a file name that Ask for changes quotes: the run chose
/// the name, so it stays a short label and not a second message.
const artifactNameShownMax = 80;

final _lineBreaks = RegExp(r'[\p{Cc}\p{Zl}\p{Zp}\s]+', unicode: true);
// Format characters: bidi overrides and isolates, zero-width marks.
final _format = RegExp(r'\p{Cf}', unicode: true);

/// A kept file's [name] as data inside a message: one line, without control or
/// bidi characters, with no double quote to end the quoting, and at most
/// [artifactNameShownMax] characters.
String artifactNameShown(String name) {
  final clean = name.replaceAll(_format, '').replaceAll(_lineBreaks, ' ').replaceAll('"', "'").trim();
  final runes = clean.runes;
  return runes.length <= artifactNameShownMax ? clean : '${String.fromCharCodes(runes.take(artifactNameShownMax - 1)).trimRight()}…';
}

/// "Ask for changes": the one message that goes back to the conversation the
/// artifact came from. The employee makes a new version of the same file. The
/// name is quoted as data (SEC5 m1): the run chose it, the board member did not.
String artifactFeedback(String name, int version, String text) => 'Feedback on the file "${artifactNameShown(name)}" (v$version): ${text.trim()}';
