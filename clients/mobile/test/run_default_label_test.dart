import 'package:aurelius_mobile/data/protocol/models.dart';
import 'package:aurelius_mobile/features/my_defaults/run_default_label.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('a run on your own login says Your default, on another board member\'s their name', () {
    expect(whoseDefault(meId: 'me'), 'Your default');
    expect(whoseDefault(loginId: 'me', loginName: 'Sam', meId: 'me'), 'Your default');
    expect(whoseDefault(loginId: 'kim-id', loginName: 'Kim', meId: 'me'), "Kim's default");
  });

  test('the label carries the resolved value once it is known', () {
    expect(defaultLabel('Your default', 'Opus 5.5'), 'Your default · Opus 5.5');
    expect(defaultLabel("Kim's default", null), "Kim's default");
    expect(resetLabel('Your default'), 'Use your default');
    expect(resetLabel("Kim's default"), "Use Kim's default");
  });

  test('labels resolve the chosen values, else the model\'s effort and Ask', () {
    const view = UserRunDefaultsView(defaults: UserRunDefaults(), model: 'claude-opus-5-5', modelDefaultEffort: 'high', supportedEfforts: []);
    final labels = RunDefaultLabels.of('Your default', view);
    expect((labels.model, labels.effort, labels.mode, labels.reset), ('Your default · Opus 5.5', 'Your default · High', 'Your default · Ask', 'Use your default'));
    const own = UserRunDefaultsView(defaults: UserRunDefaults(effort: 'low', mode: AgentMode.plan), model: 'claude-sonnet-5-5', supportedEfforts: []);
    final kim = RunDefaultLabels.of("Kim's default", own);
    expect((kim.model, kim.effort, kim.mode), ("Kim's default · Sonnet 5.5", "Kim's default · Low", "Kim's default · Plan"));
    expect(RunDefaultLabels.of('Your default', null).model, 'Your default');
  });
}
