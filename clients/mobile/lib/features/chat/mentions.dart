import 'package:flutter/material.dart';

import '../../core/theme/tokens.dart';
import '../../data/views.dart';
import '../company/company_bloc.dart';

/// An employee's name with their role, as a ticket's session shows senders
/// and `@` suggestions: "Orion — Project Manager". Without a role, the name alone.
String withRole(String name, String? role) {
  final r = role?.trim() ?? '';
  return r.isEmpty ? name : '$name — $r';
}

/// The role of the employee (not archived) called [name], from the company.
String? employeeRole(CompanyState company, String? name) {
  if (name == null) return null;
  return company.employees.where((e) => e.status != LifecycleStatus.archived && e.name == name).firstOrNull?.role;
}

/// The employees an `@` in a ticket's session can mention: not archived, with their role.
List<(String, String)> mentionables(CompanyState company) => [
      for (final e in company.employees)
        if (e.status != LifecycleStatus.archived) (e.name, e.role),
    ];

/// The `@name` being typed at the end of [text]: where its `@` is, and the
/// names that start with what follows it (case-insensitive; a name already
/// typed whole is no suggestion), at most six. An `@` inside a word (an email
/// address) starts none. As on the desktop.
(int, List<String>)? mentionSuggestions(String text, List<String> names) {
  final at = text.lastIndexOf('@');
  if (at < 0) return null;
  if (at > 0 && RegExp(r'[\p{L}\p{N}._]', unicode: true).hasMatch(text[at - 1])) return null;
  final typed = text.substring(at + 1).toLowerCase();
  if (typed.contains('\n')) return null;
  final found = names.where((n) {
    final name = n.trim().toLowerCase();
    return name.startsWith(typed) && name != typed;
  }).take(6).toList();
  return found.isEmpty ? null : (at, found);
}

/// [text] with the `@name` being typed at its end (from [at]) completed to `@<name> `.
String completeMention(String text, int at, String name) => '${text.substring(0, at)}@$name ';

/// Above a ticket session's composer: who the `@name` being typed can be, with
/// their role; a tap completes it.
class MentionRow extends StatelessWidget {
  const MentionRow({super.key, required this.found, required this.roles, required this.onPick});

  final List<String> found;
  final Map<String, String> roles;
  final ValueChanged<String> onPick;

  @override
  Widget build(BuildContext context) {
    return Padding(
      padding: const EdgeInsets.only(bottom: 6),
      child: SizedBox(
        height: 34,
        child: ListView(
          key: const Key('mentions'),
          scrollDirection: Axis.horizontal,
          children: [
            for (final name in found)
              Padding(
                padding: const EdgeInsets.only(right: 6),
                child: ActionChip(
                  key: Key('mention-$name'),
                  visualDensity: VisualDensity.compact,
                  label: Text('@${withRole(name, roles[name])}', style: TextStyle(fontSize: 13, color: Tokens.text)),
                  onPressed: () => onPick(name),
                ),
              ),
          ],
        ),
      ),
    );
  }
}
