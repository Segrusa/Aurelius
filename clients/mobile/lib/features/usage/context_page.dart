import 'package:flutter/material.dart';
import 'package:flutter_bloc/flutter_bloc.dart';

import '../../core/theme/tokens.dart';
import '../../core/widgets/widgets.dart';
import '../../data/aurelius_repository.dart';
import '../guidelines/run_guidelines.dart';
import 'usage_bloc.dart';
import '../../core/widgets/ios.dart';

/// What was considered for one answer, and why it was (not) used.
class ContextPage extends StatelessWidget {
  const ContextPage({super.key, required this.runId});

  final String runId;

  @override
  Widget build(BuildContext context) {
    return BlocProvider(
      create: (context) => ContextBloc(context.read<AureliusRepository>(), runId),
      child: Scaffold(
        appBar: GlassAppBar(title: const Text('Context')),
        body: BlocBuilder<ContextBloc, ContextState>(
          builder: (context, state) {
            if (state.loading) return const Center(child: CircularProgressIndicator());
            final view = state.view;
            if (view == null) return EmptyState('No context overview', detail: state.error);
            return ListView(
              padding: EdgeInsets.only(bottom: listBottom(context)),
              children: [
                Padding(
                  padding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
                  child: Text('${Fmt.tokens(view.usedTokens)} of ${Fmt.tokens(view.budgetTokens)} tokens', style: Tokens.title(size: 20)),
                ),
                // Which guideline versions this run had (B6, P-59).
                RunGuidelinesLine(runId: runId),
                if (view.scopes.isNotEmpty)
                  Padding(padding: const EdgeInsets.fromLTRB(16, 4, 16, 0), child: Text('Scopes: ${view.scopes.join(', ')}', style: TextStyle(color: Tokens.text2))),
                if (view.runtimeManaged)
                  Padding(
                    padding: const EdgeInsets.fromLTRB(16, 8, 16, 0),
                    child: Text('The runtime keeps its own history; Aurelius does not see every token.', style: TextStyle(color: Tokens.muted, fontSize: 12.5)),
                  ),
                const Eyebrow('Items'),
                for (final item in view.items)
                  ListTile(
                    dense: true,
                    leading: Icon(item.included ? Icons.check_circle : Icons.remove_circle_outline, size: 18, color: item.included ? Tokens.accent : Tokens.muted),
                    title: Text(item.label, maxLines: 2, overflow: TextOverflow.ellipsis),
                    subtitle: Text('${item.section} · ${item.reason}', style: TextStyle(color: Tokens.muted, fontSize: 12)),
                    trailing: Text(Fmt.tokens(item.tokens), style: Tokens.monoSmall()),
                  ),
              ],
            );
          },
        ),
      ),
    );
  }
}
