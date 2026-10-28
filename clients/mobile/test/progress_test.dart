import 'package:aurelius_mobile/core/widgets/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('how long something runs and its tokens read as on the desktop', () {
    expect(Fmt.elapsed(45000), '45s');
    expect(Fmt.elapsed(146000), '2m 26s');
    expect(Fmt.elapsed(3780000), '1h 03m');
    expect(Fmt.elapsed(-5), '0s', reason: 'a clock a little ahead never shows negative time');
    expect(Fmt.progress(1000, 3900, nowMs: 147000), '2m 26s · 3.9k tokens');
    expect(Fmt.progress(1000, 0, nowMs: 11000), '10s', reason: 'no tokens yet: only the time');
  });
}
