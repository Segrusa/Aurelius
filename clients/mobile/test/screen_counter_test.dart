import 'package:aurelius_mobile/core/usage/screen_counter.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('an opening counts, with the first and last time', () {
    final c = ScreenCounter();
    expect(c.open('/ticket/:tid', 100), isTrue);
    c.open('/home', 150);
    expect(c.open('/ticket/:tid', 200), isTrue);
    expect(
      c.stats['/ticket/:tid'],
      const ScreenStat(opens: 2, firstMs: 100, lastMs: 200),
    );
  });

  test('the same route right after itself is not counted again', () {
    final c = ScreenCounter();
    c.open('/home', 1);
    expect(c.open('/home', 2), isFalse);
    expect(c.stats['/home']!.opens, 1);
    expect(c.stats['/home']!.lastMs, 1);
  });

  test('going back to a screen is not an opening, but opening it again is', () {
    final c = ScreenCounter();
    c.open('/home', 1);
    c.open('/ticket/:tid', 2);
    c.resume('/home');
    expect(c.open('/ticket/:tid', 3), isTrue);
    expect(c.stats['/home']!.opens, 1);
    expect(c.stats['/ticket/:tid']!.opens, 2);
  });

  test('the counts survive a JSON round trip', () {
    final c = ScreenCounter()
      ..open('/home', 1)
      ..open('/board', 2)
      ..open('/home', 3);
    final back = ScreenCounter.fromJsonString(c.toJsonString());
    expect(back.stats, c.stats);
  });

  test('unreadable stored JSON gives an empty counter', () {
    expect(ScreenCounter.fromJsonString('{nope').stats, isEmpty);
    expect(ScreenCounter.fromJsonString(null).stats, isEmpty);
  });

  test('screens are ranked by opens, most recent first among equals', () {
    final c = ScreenCounter()
      ..open('/a', 1)
      ..open('/b', 2)
      ..open('/c', 3)
      ..open('/b', 4);
    expect(c.ranked().map((e) => e.key), ['/b', '/c', '/a']);
  });

  test('merging adds opens and widens the time span', () {
    final c = ScreenCounter()..open('/home', 50);
    c.merge(
      ScreenCounter({
        '/home': const ScreenStat(opens: 3, firstMs: 10, lastMs: 20),
        '/more': const ScreenStat(opens: 1, firstMs: 5, lastMs: 5),
      }),
    );
    expect(
      c.stats['/home'],
      const ScreenStat(opens: 4, firstMs: 10, lastMs: 50),
    );
    expect(c.stats['/more']!.opens, 1);
  });

  test('reset forgets every count and the current screen', () {
    final c = ScreenCounter()..open('/home', 1);
    c.reset();
    expect(c.stats, isEmpty);
    expect(c.open('/home', 2), isTrue);
  });
}
