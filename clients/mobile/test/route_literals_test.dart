import 'dart:io';

import 'package:aurelius_mobile/app/router.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/fake_daemon.dart' show testHost;

/// A place the app navigates to, as written in a string literal in lib/.
class NavigationPlace {
  const NavigationPlace(this.file, this.line, this.literal);

  final String file;
  final int line;
  final String literal;

  @override
  String toString() => '$file:$line  $literal';
}

final _call = RegExp(r'''\.(push|go|pushReplacement|replace)(<[^>]*>)?\(\s*'(?=/)''');

/// Every string literal passed to push/go/pushReplacement/replace under lib/
/// (the generated protocol excluded), with its file and line.
List<NavigationPlace> navigationLiterals(Directory lib) {
  final places = <NavigationPlace>[];
  final files = lib.listSync(recursive: true).whereType<File>().where((f) => f.path.endsWith('.dart') && !f.path.contains('data${Platform.pathSeparator}protocol'));
  for (final file in files) {
    final source = file.readAsStringSync();
    for (final m in _call.allMatches(source)) {
      final literal = readLiteral(source, m.end);
      if (literal == null) continue;
      places.add(NavigationPlace(file.path, '\n'.allMatches(source.substring(0, m.start)).length + 1, literal));
    }
  }
  return places;
}

/// The single-quoted literal that starts at [start] (just after its quote),
/// with `${...}` taken as a whole: it may hold quotes of its own.
String? readLiteral(String source, int start) {
  var depth = 0;
  for (var i = start; i < source.length; i++) {
    final c = source[i];
    if (depth == 0 && c == "'") return source.substring(start, i);
    if (c == r'$' && i + 1 < source.length && source[i + 1] == '{') {
      depth++;
      i++;
    } else if (depth > 0 && c == '}') {
      depth--;
    } else if (depth == 0 && c == '\n') {
      return null;
    }
  }
  return null;
}

/// [literal] with each interpolation filled with a sample id. An interpolation
/// glued to a word ('/board/new$query') is a suffix such as a query: left out.
String sampleLocation(String literal) => literal.replaceAllMapped(RegExp(r'\$\{(?:[^{}]|\{[^{}]*\})*\}|\$[A-Za-z_]\w*'), (m) {
      final before = m.start == 0 ? '' : literal[m.start - 1];
      return before == '/' || before == '=' ? 'x1' : '';
    });

void main() {
  test('sample locations fill ids and leave a glued suffix out', () {
    expect(sampleLocation(r'/ticket/${ticket.id}/plan'), '/ticket/x1/plan');
    expect(sampleLocation(r"/conversation/$c?e=${root.employeeId ?? ''}"), '/conversation/x1?e=x1');
    expect(sampleLocation(r'/board/new$query'), '/board/new');
    expect(readLiteral(r"""/a/${b ?? ''}/c' rest""", 0), r"/a/${b ?? ''}/c");
  });

  test('every place the app pushes or goes to in lib/ is a route of the router (OPR-2)', () {
    final router = AppRouter(testHost);
    addTearDown(router.dispose);
    final places = navigationLiterals(Directory('lib'));
    expect(places.length, greaterThan(50), reason: 'the scan finds the navigation calls');
    final unknown = [
      for (final p in places)
        if (router.config.configuration.findMatch(Uri.parse(sampleLocation(p.literal))).isError) p,
    ];
    expect(unknown, isEmpty, reason: 'these open the go_router error page');
  });
}
