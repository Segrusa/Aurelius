import 'dart:math';

import 'package:aurelius_mobile/data/views.dart';
import 'package:aurelius_mobile/features/artifact/artifact_html.dart';
import 'package:aurelius_mobile/features/artifact/artifact_versions.dart';
import 'package:flutter_test/flutter_test.dart';

FileRef fileOf(String id, {String name = 'pricing.html', String mediaType = 'text/html'}) => FileRef(id: id, mediaType: mediaType, name: name, size: 1200);

MessageView keptBy(String id, int ordinal, List<FileRef> files, {int at = 0, int? deletedAtMs}) => MessageView(
      activity: const [],
      createdAtMs: at,
      deletedAtMs: deletedAtMs,
      files: files,
      id: id,
      ordinal: ordinal,
      queued: false,
      role: MessageRole.assistant,
      sources: const [],
      status: MessageStatus.complete,
      text: 'Saved design/pricing.html',
      uploads: const [],
    );

const nonce = '00112233445566778899aabbccddeeff';

void main() {
  group('versions', () {
    test('an HTML file is an artifact, also with a charset; others are not', () {
      expect(isArtifact(fileOf('a')), isTrue);
      expect(isArtifact(fileOf('a', mediaType: 'Text/HTML; charset=utf-8')), isTrue);
      expect(isArtifact(fileOf('a', mediaType: 'text/plain')), isFalse);
      expect(isArtifact(fileOf('a', mediaType: 'image/png')), isFalse);
    });

    test('versions are the same name in message order, newest first', () {
      final messages = [
        keptBy('m3', 3, [fileOf('f3')], at: 30),
        keptBy('m1', 1, [fileOf('f1')], at: 10),
        keptBy('m2', 2, [fileOf('other', name: 'home.html'), fileOf('f2')], at: 20),
      ];
      final versions = artifactVersions(messages, 'pricing.html');
      expect([for (final v in versions) (v.version, v.file.id, v.messageId, v.atMs)], [(3, 'f3', 'm3', 30), (2, 'f2', 'm2', 20), (1, 'f1', 'm1', 10)]);
    });

    test('a deleted message no longer offers its file, and the numbers close up', () {
      final messages = [keptBy('m1', 1, [fileOf('f1')]), keptBy('m2', 2, [fileOf('f2')], deletedAtMs: 5), keptBy('m3', 3, [fileOf('f3')])];
      expect([for (final v in artifactVersions(messages, 'pricing.html')) (v.version, v.file.id)], [(2, 'f3'), (1, 'f1')]);
    });

    test('two files with the same name in one message are two versions', () {
      final messages = [keptBy('m1', 1, [fileOf('a'), fileOf('b')])];
      expect([for (final v in artifactVersions(messages, 'pricing.html')) (v.version, v.file.id)], [(2, 'b'), (1, 'a')]);
    });

    test('a non-HTML file with the same name is no version', () {
      final messages = [keptBy('m1', 1, [fileOf('a', mediaType: 'text/plain')]), keptBy('m2', 2, [fileOf('b')])];
      expect([for (final v in artifactVersions(messages, 'pricing.html')) v.file.id], ['b']);
    });

    test('a file whose message is not read yet stands alone as v1', () {
      final versions = versionsFor([keptBy('m1', 1, [fileOf('f1')])], fileOf('new'), 99);
      expect(versions.single.version, 1);
      expect(versions.single.file.id, 'new');
      expect(versions.single.atMs, 99);
    });

    test('"v2 of 3" only when there is more than one', () {
      final messages = [keptBy('m1', 1, [fileOf('f1')]), keptBy('m2', 2, [fileOf('f2')]), keptBy('m3', 3, [fileOf('f3')])];
      final versions = artifactVersions(messages, 'pricing.html');
      expect(versionOf(versions, 'f2'), 'v2 of 3');
      expect(versionOf(versions.sublist(2), 'f1'), isNull);
      expect(versionOf(versions, 'unknown'), isNull);
    });

    test('the label has the time today and the date before', () {
      final at = DateTime(2026, 11, 22, 14, 5);
      final v = ArtifactVersion(version: 3, file: fileOf('f'), messageId: 'm', atMs: at.millisecondsSinceEpoch);
      expect(versionLabel(v, DateTime(2026, 11, 22, 18)), 'v3 · 14:05');
      expect(versionLabel(v, DateTime(2026, 11, 23, 9)), 'v3 · Nov 22, 14:05');
    });

    test('the widths are those of the desktop', () {
      expect([for (final v in ArtifactViewport.values) (v.label, v.width)], [('Desktop', 1280), ('Tablet', 768), ('Mobile', 390)]);
    });
  });

  group('Ask for changes', () {
    test('the message names the file and its version, then the text', () {
      expect(artifactFeedback('pricing.html', 3, '  Make the CTA green.\n'), 'Feedback on pricing.html v3: Make the CTA green.');
    });
  });

  group('html with nonce', () {
    test('a nonce is 128 bits of hex, and two are not the same', () {
      final a = artifactNonce();
      expect(a, matches(RegExp(r'^[0-9a-f]{32}$')));
      expect(artifactNonce(), isNot(a));
      expect(artifactNonce(Random(1)), artifactNonce(Random(1)));
    });

    test('every script of the artifact gets the nonce, in any case and spacing', () {
      final doc = artifactDocument('<script>a()</script><SCRIPT>b()</SCRIPT><ScRiPt\n>c()</ScRiPt>', nonce);
      expect('<script nonce=$nonce>a()'.allMatches(doc).length, 1);
      expect(doc, contains('<script nonce=$nonce>b()'));
      expect(doc, contains('<script nonce=$nonce\n>c()'));
      expect(RegExp(r'<script(?! nonce=)', caseSensitive: false).hasMatch(doc), isFalse);
    });

    test('a script with its own nonce gets ours too, so it has two and is refused', () {
      expect(artifactDocument('<script nonce="x">a()</script>', nonce), contains('<script nonce=$nonce nonce="x">'));
    });

    test('the policies, without sandbox, and the WebRTC prelude come first', () {
      final doc = artifactDocument('<p>hi</p>', nonce);
      expect(doc, startsWith('<meta http-equiv="Content-Security-Policy" content="$artifactCsp; script-src \'nonce-$nonce\'; script-src-attr \'unsafe-inline\'">'));
      expect(doc, contains('<meta http-equiv="Content-Security-Policy" content="script-src \'unsafe-inline\'">'));
      expect(doc, contains('<meta http-equiv="x-dns-prefetch-control" content="off">'));
      expect(doc.indexOf('x-dns-prefetch-control'), lessThan(doc.indexOf('<p>hi</p>')));
      expect(doc, isNot(contains('sandbox')));
      expect(doc, isNot(contains("script-src 'self'")));
      expect(doc, contains("connect-src 'none'"));
      expect(doc, contains("frame-src 'none'"));
      final prelude = doc.indexOf('<script nonce=$nonce>(function(){');
      expect(prelude, greaterThan(0));
      expect(prelude, lessThan(doc.indexOf('<p>hi</p>')));
      for (final name in artifactWebRtcNames) {
        expect(doc, contains("'$name'"));
      }
    });

    test('the first policy has no unsafe-inline for script elements: javascript: URLs stay refused', () {
      final first = artifactScriptCsp(nonce);
      expect(first, contains("script-src 'nonce-$nonce';"));
      expect(RegExp(r"script-src [^;]*'unsafe-inline'").hasMatch(first), isFalse);
    });

    test('the policy goes after the doctype, so the page keeps its standards mode', () {
      final doc = artifactDocument('﻿  <!DOCTYPE html>\n<html><head><title>x</title></head></html>', nonce);
      expect(doc, startsWith('﻿  <!DOCTYPE html><meta http-equiv="Content-Security-Policy"'));
      expect(doc.indexOf('<html>'), greaterThan(doc.indexOf('Content-Security-Policy')));
    });

    test('a nonce that is not hex is refused, so nothing gets into the policy through it', () {
      for (final bad in ['', 'abc', "00112233445566778899aabbccddeeff' 'unsafe-inline", 'ABCDEF0123456789ABCD', '0011223344556677"><script>']) {
        expect(() => artifactDocument('<p></p>', bad), throwsArgumentError, reason: bad);
      }
    });
  });
}
