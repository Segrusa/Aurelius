import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/widgets/ios.dart';
import '../../data/aurelius_repository.dart';
import '../chat/chat_bloc.dart';
import '../chat/chat_view.dart';
import '../company/company_bloc.dart';

/// The company's group chat (Team): the user and every employee. `@Name`
/// asks those employees; without one the head of the company answers.
class TeamPage extends StatelessWidget {
  const TeamPage({super.key});

  @override
  Widget build(BuildContext context) {
    final company = context.read<CompanyBloc>();
    return Scaffold(
      appBar: GlassAppBar(title: const Text('Team')),
      body: BlocProvider(
        // The employee at work on a run: who writes a streaming answer.
        create: (_) => ChatBloc.group(context.read<AureliusRepository>(), runAuthor: (runId) => runAuthorOf(company.state, runId)),
        child: const ChatView(
          key: ValueKey('team-chat'),
          emptyTitle: 'Talk to your team',
          emptyDetail: 'Write @Name to ask someone; otherwise the head of the company answers.',
        ),
      ),
    );
  }
}

/// The name of the employee whose agent runs [runId], if one does now.
String? runAuthorOf(CompanyState company, String runId) =>
    company.employees.where((m) => m.agents.any((a) => a.runId == runId)).firstOrNull?.name;
