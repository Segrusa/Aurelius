import 'protocol/models.dart';

/// The server understands `@Everyone` and `discussion.audience` (ticket #40).
/// Without it the word is plain text: no suggestion, no mark, no count.
const everyoneFeature = 'everyone_mention';

/// The word written after the `@`, as the suggestions and messages show it.
const everyoneName = 'Everyone';

/// What the `@Everyone` suggestion says it is, in the place of a role.
const everyoneRole = 'everyone in this discussion';

/// How long the composer waits after the text turns to contain `@Everyone`
/// before it asks who that reaches.
const everyoneAudienceDelayMs = 400;

/// Whether `@Everyone` is mentioned in [text]: the whole word, in any case
/// (`@everyone,` counts, `@everyone2` and an email address do not).
bool mentionsEveryone(String text) =>
    RegExp(r'(?<![\p{L}\p{N}._])@everyone(?![\p{L}\p{N}_-])', caseSensitive: false, unicode: true).hasMatch(text);

/// [names] with `Everyone` first, when the server has the feature [enabled].
List<String> withEveryone(List<String> names, {required bool enabled}) => enabled ? [everyoneName, ...names] : names;

/// The line above the composer saying whom `@Everyone` reaches, e.g.
/// "Orion picks who follows up; 5 employees can read along (1 paused), 2 board
/// members are told". Without a coordinator (an older server, which wakes them
/// all): "Wakes 5 employees (1 paused) and notifies 2 board members".
String everyoneAudienceLine(DiscussionAudience a) {
  String plural(int n, String one, String many) => '$n ${n == 1 ? one : many}';
  final coordinator = a.coordinator;
  if (coordinator != null && coordinator.isNotEmpty) {
    final paused = a.paused > 0 ? ' (${a.paused} paused)' : '';
    final told = a.boardMembers > 0 ? ', ${plural(a.boardMembers, 'board member', 'board members')} ${a.boardMembers == 1 ? 'is' : 'are'} told' : '';
    return '$coordinator picks who follows up; ${plural(a.employees, 'employee', 'employees')} can read along$paused$told';
  }
  final employees = 'Wakes ${plural(a.employees, 'employee', 'employees')}${a.paused > 0 ? ' (${a.paused} paused)' : ''}';
  return a.boardMembers > 0 ? '$employees and notifies ${plural(a.boardMembers, 'board member', 'board members')}' : employees;
}
