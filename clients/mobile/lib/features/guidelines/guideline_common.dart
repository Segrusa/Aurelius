import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/theme/tokens.dart';
import '../../data/aurelius_repository.dart';
import '../../data/guidelines.dart';
import '../company/company_bloc.dart';
import '../connection/connection_bloc.dart';
import 'guidelines_bloc.dart';

/// Names for ids, from the company the phone knows.
class Names {
  const Names(this.company);

  factory Names.of(BuildContext context) => Names(context.read<CompanyBloc>().state);

  final CompanyState company;

  String employee(String id) => company.employee(id)?.name ?? 'an employee';
  String project(String id) => company.project(id)?.name ?? 'a project';

  /// The refusal or failure of a guideline write in words.
  String error(Object e, {String? title}) => guidelineError(e, title: title, employee: employee, project: project);
}

/// The server has guidelines (feature `guidelines`); false where no connection is known.
bool guidelinesOn(BuildContext context) {
  try {
    return context.read<ConnectionBloc>().state.has('guidelines');
  } on ProviderNotFoundException {
    return false;
  }
}

/// [child] with the screen's [GuidelinesBloc], or a new one when it was opened
/// from elsewhere (an employee, a project, a run, the Inbox).
Widget withGuidelines(BuildContext context, Widget child) {
  GuidelinesBloc? bloc;
  try {
    bloc = context.read<GuidelinesBloc>();
  } on ProviderNotFoundException {
    bloc = null;
  }
  return bloc == null
      ? BlocProvider(create: (c) => GuidelinesBloc(c.read<AureliusRepository>()), child: child)
      : BlocProvider.value(value: bloc, child: child);
}

/// The counter and its bar: neutral, orange from 80 %, red over the limit, always with words (ui §4.1, §9).
class LimitBar extends StatelessWidget {
  const LimitBar({super.key, required this.count, this.max = guidelineMaxChars, this.label});

  final int count;
  final int max;

  /// Before the numbers ("Largest block: Bridge in aurelius · ").
  final String? label;

  @override
  Widget build(BuildContext context) {
    final level = limitLevel(count, max);
    final color = switch (level) { LimitLevel.ok => Tokens.gray, LimitLevel.close => Tokens.warning, LimitLevel.over => Tokens.negative };
    final note = limitNote(count, max: max);
    return Semantics(
      label: [counterLabel(count, max: max), ?note].join(', '),
      excludeSemantics: true,
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          Row(children: [
            Expanded(
              child: Text(
                '${label ?? ''}${counterLine(count, max: max)}',
                maxLines: 2,
                style: TextStyle(fontSize: 13, color: level == LimitLevel.ok ? Tokens.label2 : color, fontFeatures: Tokens.tabular),
              ),
            ),
          ]),
          const SizedBox(height: 4),
          ClipRRect(
            borderRadius: BorderRadius.circular(2),
            child: LinearProgressIndicator(value: (count / max).clamp(0.0, 1.0), minHeight: 4, color: color, backgroundColor: Tokens.fill),
          ),
          if (note != null)
            Padding(
              padding: const EdgeInsets.only(top: 3),
              child: Text(note, style: TextStyle(fontSize: 12.5, color: color)),
            ),
        ],
      ),
    );
  }
}
