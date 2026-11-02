import 'package:aurelius_mobile/core/hosts/host.dart';
import 'package:aurelius_mobile/core/net/protocol.dart';
import 'package:aurelius_mobile/data/commands.dart';
import 'package:aurelius_mobile/data/line_diff.dart';
import 'package:aurelius_mobile/data/org_chart.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:flutter_test/flutter_test.dart';

import 'support/builders.dart';

/// Wire shapes of protocol 3.0 (the company), parsed with the generated types.
ResponseBody body(Map<String, dynamic> json) {
  final message = ServerMessage.fromJson({'type': 'response', 'request_id': 7, 'ok': json}) as ServerMessageResponse;
  return message.ok!;
}

void main() {
  test('every request names its method <area>.<verb> and carries the major', () {
    expect(protocolMajor, 5);
    final request = Commands.sendProjectMessage('p', 'Hello');
    expect(request['method'], 'project_conversation.send');
    expect((request['params'] as Map)['text'], 'Hello');
    expect(Commands.setReportsTo('m', null)['method'], 'employee.set_reports_to');
    expect(Commands.decideProposal('p', true)['method'], 'proposal.decide');
    // A company setting now, not a project's.
    expect(Commands.setFollowOrgChart(true), {
      'method': 'company.set_follow_org_chart',
      'params': {'follow': true},
    });
  });

  test('employees are written company-wide', () {
    expect(Commands.createEmployee('Ada', 'CTO', reportsTo: 'm0'), {
      'method': 'employee.create',
      'params': {'name': 'Ada', 'reports_to': 'm0', 'role': 'CTO'},
    });
    expect(Commands.sendEmployeeMessage('m', 'Hi', conversationId: 'chat')['method'], 'employee.send_message');
    expect(paramsOf(Commands.assignTask('t', 'm')), {'employee_id': 'm', 'task_id': 't'});
    expect(paramsOf(Commands.remember('employee', 'm', MemoryKind.decision, 'x'))['scope'], {'kind': 'employee', 'id': 'm'});
  });

  test('company.get: employees, the setting and the management team', () {
    final company = (body({
      'type': 'company',
      'value': {
        'as_of_seq': 4,
        'company': {'id': 'co', 'name': 'Segrusa', 'version': 2, 'dispatcher_employee_id': 'm1'},
        'employees': [
          {
            'id': 'm1', 'name': 'Ada', 'role': 'CTO', 'status': 'active', 'version': 1, 'conversation_id': 'c1', 'working': false,
            'agent_mode': 'ask', 'browser_allowed': false, 'screen_allowed': false, 'sequential_tasks': false,
            'conversations': [], 'agents': [], 'projects_active': ['p'],
          },
        ],
        'follow_org_chart': true,
        'roots': ['m1'],
      },
    }) as ResponseBodyCompany)
        .value;
    expect(company.company.name, 'Segrusa');
    expect(company.company.dispatcherEmployeeId, 'm1');
    expect(company.followOrgChart, isTrue);
    expect(company.roots, ['m1']);
    expect(company.employees.single.projectsActive, ['p']);
  });

  test('hello answers the major and the rights', () {
    final hello = (body({
      'type': 'hello',
      'value': {
        'capabilities': ['read', 'chat', 'approve'],
        'daemon_version': '0.7.0',
        'environment': 'production',
        'host_id': 'h',
        'host_name': 'my-pc',
        'os': 'windows',
        'protocol': 5,
        'remote': true,
      },
    }) as ResponseBodyHello)
        .value;
    expect(hello.protocol, protocolMajor);
    expect(hello.capabilities.map((c) => c.wire), ['read', 'chat', 'approve']);
  });

  test('projects count the employees active in them', () {
    final projects = (body({
      'type': 'projects',
      'value': {
        'as_of_seq': 1,
        'projects': [
          {'id': 'p', 'name': 'Atlas', 'status': 'active', 'version': 2, 'created_at_ms': 1, 'employees_active': 2, 'folder': r'C:\Projects\Atlas'},
        ],
      },
    }) as ResponseBodyProjects)
        .value
        .projects;
    expect(projects.single.employeesActive, 2);
    expect(projects.single.status, ProjectStatus.active);
    expect(projects.single.folder, r'C:\Projects\Atlas');
  });

  test('employees with a title, a boss, heartbeat and what runs for them', () {
    final m = (body({
      'type': 'employees',
      'value': {
        'as_of_seq': 1,
        'employees': [
          {
            'id': 'm1',
            'projects_active': ['p'],
            'name': 'Iris',
            'role': 'Writer',
            'title': 'Head of content',
            'reports_to': 'm0',
            'status': 'active',
            'version': 3,
            'conversation_id': 'main',
            'working': true,
            'agent_mode': 'plan',
            'browser_allowed': true,
            'screen_allowed': false,
            'sequential_tasks': true,
            'conversations': [
              {'id': 'c1', 'title': 'Launch', 'created_at_ms': 1, 'updated_at_ms': 2, 'message_count': 3},
            ],
            'heartbeat': {'every_minutes': 60, 'quiet_start_min': 0, 'quiet_end_min': 0, 'daily_token_budget': 0, 'tokens_today': 0},
            'agents': [
              {'kind': 'planning', 'name': 'Iris', 'detail': '', 'since_ms': 1, 'steps': 0},
              {'kind': 'subagent', 'name': 'Explore', 'detail': '', 'since_ms': 1, 'steps': 3, 'doing': 'Read · hero.html', 'prompt': 'Compare the two hero layouts.'},
              {'kind': 'delegated', 'name': 'Bram', 'detail': '', 'since_ms': 1, 'steps': 0, 'task_id': 't9'},
            ],
          },
        ],
      },
    }) as ResponseBodyEmployees)
        .value
        .employees
        .single;
    expect(m.title, 'Head of content');
    expect(m.reportsTo, 'm0');
    expect(m.roleLine, 'Head of content · Writer');
    expect(m.working, isTrue);
    expect(m.agentMode, AgentMode.plan);
    expect(m.heartbeat!.everyMinutes, 60);
    expect(m.monogram, 'I');
    expect(m.threadTitle('c1'), 'Launch');
    expect(m.threadTitle('other'), 'New conversation');
    expect(m.agents.map((a) => a.kind), ['planning', 'subagent', 'delegated']);
    expect(m.agents[1].label, 'Subagent');
    expect(m.agents[1].doing, 'Read · hero.html');
    expect(m.agents[2].taskId, 't9');
  });

  test('messages with a streaming answer, sources, files and tool use at a byte offset', () {
    final m = (body({
      'type': 'messages',
      'value': {
        'as_of_seq': 50,
        'conversation_id': 'c',
        'agent_mode': 'accept_edits',
        'browser_allowed': false,
        'screen_allowed': false,
        'run': <String, dynamic>{},
        'attachments': [
          {'id': 'a', 'name': 'note.txt', 'kind': 'text', 'byte_length': 10, 'truncated': false},
        ],
        'shares': [],
        'messages': [
          {
            'id': 'm1', 'ordinal': 1, 'role': 'user', 'status': 'complete', 'text': 'Hello', 'created_at_ms': 1, 'queued': false,
            'sources': [], 'activity': [], 'files': [],
            'uploads': [
              {'name': 'report.pdf', 'path': '.aurelius/uploads/report.pdf', 'byte_length': 1258291},
            ],
          },
          {
            'id': 'm2', 'ordinal': 2, 'role': 'assistant', 'status': 'partial', 'text': 'Hi 👋', 'created_at_ms': 2, 'queued': false,
            'author': 'Marcus',
            'sources': [
              {'n': 1, 'url': 'https://example.com', 'title': 'Example'},
            ],
            'activity': [
              {'call_id': 't', 'tool': 'Bash', 'input': {'command': 'cargo test'}, 'output': 'ok', 'ok': true, 'offset': 3},
            ],
            'files': [
              {'id': 'f', 'name': 'scherm1.png', 'media_type': 'image/png', 'size': 1484481},
            ],
            'uploads': [],
          },
        ],
      },
    }) as ResponseBodyMessages)
        .value;
    expect(m.messages, hasLength(2));
    expect(m.messages.first.isUser, isTrue);
    expect(m.messages.first.uploads.single.name, 'report.pdf');
    final answer = m.messages.last;
    expect(answer.isStreaming, isTrue);
    expect(answer.files.single.isImage, isTrue);
    expect(answer.files.single.sizeLabel, '1.4 MB');
    expect(answer.author, 'Marcus');
    expect(answer.sources.single.title, 'Example');
    final tool = answer.activity.single;
    expect(tool.tool, 'Bash');
    expect(tool.field('command'), 'cargo test');
    expect(tool.ok, isTrue);
    // The byte offset 3 falls after "Hi ", before the emoji.
    expect(answer.parts, ['Hi ', tool, '👋']);
    expect(m.attachments.single.name, 'note.txt');
    expect(m.agentMode, AgentMode.acceptEdits);
    expect(m.browserAllowed, isFalse, reason: 'the browser is off unless the user allowed it');
  });

  test('UTF-8 byte offsets map to UTF-16 indexes', () {
    expect(utf16Index('Hi 👋 there', 3), 3);
    // The emoji is 4 bytes in UTF-8 and 2 units in UTF-16.
    expect(utf16Index('Hi 👋 there', 7), 5);
    // Inside a character rounds down; past the end clamps.
    expect(utf16Index('Hi 👋 there', 5), 3);
    expect(utf16Index('é', 99), 1);
  });

  test('a task waiting for you, a sub-task and its labels', () {
    final t = (body({
      'type': 'tasks',
      'value': {
        'as_of_seq': 1,
        'tasks': [
          {
            'id': 't', 'project_id': 'p', 'employee_id': 'm', 'employee_name': 'Iris', 'title': 'Landing page', 'goal': 'Ship it', 'criteria': ['Fast'],
            'status': 'waiting', 'waiting_reason': 'input', 'created_at_ms': 1, 'updated_at_ms': 2, 'depends_on': [], 'pending_approvals': 1,
            'parent_task_id': 'parent', 'delegated_by': 'Bram', 'children': [], 'conversation_id': 'conv',
            'handoffs': [
              {'from': 'Bram', 'to': 'Iris', 'reason': 'Better fit'},
            ],
            'run': {'mode': 'auto', 'model': 'claude-sonnet-5-5', 'effort': 'high'},
            'attachments': [],
          },
        ],
      },
    }) as ResponseBodyTasks)
        .value
        .tasks
        .single;
    expect(t.needsYou, isTrue);
    expect(t.statusLabel, 'Waiting for you');
    expect(t.delegatedBy, 'Bram', reason: 'a sub-task knows who delegated it');
    expect(t.handoffs.single.to, 'Iris');
    expect(t.canAssign, isTrue);
    expect(t.conversationId, 'conv');
    expect(t.run.mode, AgentMode.auto);
    expect(t.run.choices, {'mode': 'auto', 'model': 'claude-sonnet-5-5', 'effort': 'high'});
    expect(t.pendingApprovals, 1);
  });

  test("Claude's questions arrive as approvals", () {
    final q = (body({
      'type': 'approvals',
      'value': {
        'as_of_seq': 1,
        'approvals': [
          {
            'id': 'a', 'status': 'pending', 'tool': 'claude.AskUserQuestion', 'effect': 'read', 'account': '', 'reasons': [], 'resources': [],
            'action_digest': 'abcdef0123456789', 'requested_at_ms': 1, 'expires_at_ms': 2, 'run_id': 'r', 'requester': 'Iris · Copy', 'task_id': 't',
            'arguments': {
              'questions': [
                {
                  'question': 'Which tone?', 'header': 'Tone', 'multiSelect': false,
                  'options': [
                    {'label': 'Formal', 'description': 'Serious'},
                    {'label': 'Playful', 'description': 'Light'},
                  ],
                },
              ],
            },
          },
        ],
      },
    }) as ResponseBodyApprovals)
        .value
        .approvals
        .single;
    expect(q.isQuestion, isTrue);
    expect(q.isPending, isTrue);
    expect(q.title, 'Iris · Copy asks you');
    expect(q.taskId, 't');
    expect(q.shortDigest, 'abcdef012345');
    final question = q.questions.single;
    expect(question.text, 'Which tone?');
    expect(question.multi, isFalse);
    expect(question.options.map((o) => o.label), ['Formal', 'Playful']);
    expect(Commands.answerQuestion(q.id, {'Which tone?': 'Playful'})['params'], {'answers': {'Which tone?': 'Playful'}, 'approval_id': 'a'});
  });

  test('colleague proposals', () {
    final p = (body({
      'type': 'proposals',
      'value': {
        'as_of_seq': 1,
        'proposals': [
          {
            'id': 'pr', 'project_id': 'p', 'proposed_by': 'm1', 'proposed_by_name': 'Iris', 'name': 'Sam', 'role': 'Designer', 'title': 'Lead designer',
            'instructions': 'Design', 'reason': 'We need it', 'reports_to': 'm1', 'reports_to_name': 'Iris', 'status': 'pending', 'created_at_ms': 1,
          },
        ],
      },
    }) as ResponseBodyProposals)
        .value
        .proposals
        .single;
    expect(p.isPending, isTrue);
    expect(p.reportsToName, 'Iris');
    expect(p.status, ProposalStatus.pending);
    expect(Commands.decideProposal(p.id, false, note: 'Not now'), {
      'method': 'proposal.decide',
      'params': {'approve': false, 'note': 'Not now', 'proposal_id': 'pr'},
    });
  });

  group('Harness²', () {
    test('a lesson is an episodic memory with when it applies and its evidence (task_lessons)', () {
      final items = (body({
        'type': 'memory',
        'value': {
          'items': [
            {
              'id': 'mem1', 'scope': 'employee', 'employee_id': 'm1', 'employee_name': 'Iris', 'kind': 'episodic', 'status': 'active',
              'content': 'Run the tests before saying done.', 'confidence': 0.8, 'importance': 0.6, 'pinned': false, 'created_at_ms': 1,
              'sources': [
                {'event_id': 'e1', 'label': 'Run failed', 'wire': 'run.failed', 'at_ms': 1},
              ],
              'lesson': {
                'applies_when': 'changing Rust code',
                'evidence': [
                  {'label': 'S1', 'text': 'User: the build is red again'},
                  {'label': 'S3', 'text': 'Run failed: cargo test'},
                ],
                'reinforced': 2,
                'tasks': ['t1', 't2'],
              },
            },
            {
              'id': 'mem2', 'scope': 'project', 'kind': 'decision', 'status': 'active', 'content': 'Use Postgres.', 'confidence': 1, 'importance': 1,
              'pinned': true, 'created_at_ms': 2, 'sources': [],
            },
          ],
        },
      }) as ResponseBodyMemory)
          .value
          .items;
      final lesson = items.first.lesson!;
      expect(items.first.kind, MemoryKind.episodic, reason: 'kind stays episodic');
      expect(lesson.appliesWhen, 'changing Rust code');
      expect(lesson.evidence.map((e) => e.label), ['S1', 'S3']);
      expect(lesson.confirmedLine, 'confirmed 2×');
      expect(lesson.tasks, ['t1', 't2']);
      expect(items.first.sources.single.label, 'Run failed', reason: 'new source labels are shown as is');
      expect(items.last.lesson, isNull, reason: 'older memories and older servers have no lesson');
      expect(const LessonLocator(appliesWhen: 'x').confirmedLine, isNull);

      final reinforced = (ServerMessage.fromJson({'type': 'event', 'seq': 5, 'kind': 'memory.reinforced', 'aggregate_id': 'mem1', 'at_ms': 3}) as ServerMessageEvent).value;
      expect(reinforced.kind, EventKind.memoryReinforced);
      expect(reinforced.isMemory, isTrue, reason: 'the memory list refreshes on it');
    });

    test('proposed instructions carry the diff base, the evidence and a stale version (instruction_proposals)', () {
      final proposals = (body({
        'type': 'proposals',
        'value': {
          'as_of_seq': 1,
          'proposals': [
            {
              'id': 'pr2', 'proposed_by': 'm1', 'proposed_by_name': 'Iris', 'name': 'Max', 'role': 'Developer',
              'instructions': 'Be brief.\nRun the tests.', 'reason': 'Two failed tasks', 'status': 'pending', 'created_at_ms': 1,
              'kind': 'instructions', 'target_employee_id': 'm2', 'target_employee_name': 'Max', 'base_version': 3,
              'current_instructions': 'Be brief.', 'current_version': 4,
              'evidence': [
                {'kind': 'task', 'id': 't7', 'label': 'Fix the build'},
              ],
            },
            {
              'id': 'pr1', 'proposed_by': 'm1', 'proposed_by_name': 'Iris', 'name': 'Sam', 'role': 'Designer',
              'instructions': 'Design', 'reason': 'We need it', 'status': 'pending', 'created_at_ms': 1,
            },
          ],
        },
      }) as ResponseBodyProposals)
          .value
          .proposals;
      final p = proposals.first;
      expect(p.isInstructions, isTrue);
      expect(p.targetEmployeeName, 'Max');
      expect(p.evidence.single.label, 'Fix the build');
      expect(p.isStale, isTrue, reason: 'current_version 4 != base_version 3');
      expect(proposals.last.isInstructions, isFalse, reason: 'absent kind is a colleague');
      expect(proposals.last.isStale, isFalse);
      expect(instructionProposalOf('x').isStale, isFalse);

      expect(Commands.decideProposal('pr2', true, instructions: 'Be brief.\nRun all tests.'), {
        'method': 'proposal.decide',
        'params': {'approve': true, 'instructions': 'Be brief.\nRun all tests.', 'proposal_id': 'pr2'},
      });
      expect(Commands.decideProposal('pr2', true)['params'], {'approve': true, 'proposal_id': 'pr2'}, reason: 'unedited sends no instructions');
      final proposed = (ServerMessage.fromJson({'type': 'event', 'seq': 6, 'kind': 'employee.instructions_proposed', 'aggregate_id': 'pr2', 'at_ms': 3}) as ServerMessageEvent).value;
      expect(proposed.isChat, isFalse, reason: 'company changes refresh the inbox');
    });

    test('the line diff between current and proposed instructions', () {
      final diff = lineDiff('Be brief.\nShip on Fridays.\nSay hi.', 'Be brief.\nRun the tests.\nSay hi.');
      expect(diff, [
        (op: DiffOp.same, text: 'Be brief.'),
        (op: DiffOp.removed, text: 'Ship on Fridays.'),
        (op: DiffOp.added, text: 'Run the tests.'),
        (op: DiffOp.same, text: 'Say hi.'),
      ]);
      expect(lineDiff('', 'New'), [(op: DiffOp.added, text: 'New')], reason: 'an employee without instructions');
      expect(lineDiff('a\nb', 'a\nb').every((l) => l.op == DiffOp.same), isTrue);
    });

    test('variants: the run field, the compare view and the choice (task_variants)', () {
      // 1 sends nothing, as servers without the feature expect.
      expect(paramsOf(Commands.createTask('p', 'T', 'G', const [])).containsKey('run'), isFalse);
      expect(paramsOf(Commands.createTask('p', 'T', 'G', const [], run: {'mode': 'auto'}, variants: 3))['run'], {'mode': 'auto', 'variants': 3});
      expect(paramsOf(Commands.startIdea('i', 'm', variants: 2))['run'], {'variants': 2});
      expect(paramsOf(Commands.startIdea('i', 'm', mode: AgentMode.plan))['run'], {'mode': 'plan'});
      expect(Commands.chooseVariant('t', variant: 2), {
        'method': 'task.choose_variant',
        'params': {'task_id': 't', 'variant': 2},
      });
      expect(Commands.chooseVariant('t')['params'], {'task_id': 't'}, reason: "absent: the judge's pick");
      expect(variantsCostNote(1), 'One run, as usual.');
      expect(variantsCostNote(3), contains('3×'));

      final view = (body({
        'type': 'task_variants',
        'value': {
          'task_id': 't', 'phase': 'ready', 'count': 2, 'base_commit': 'abc', 'judge_pick': 2, 'compose': 'Take 2.',
          'variants': [
            {
              'n': 1, 'hint': 'Small patch', 'status': 'done', 'files': 2, 'insertions': 10, 'deletions': 3, 'diff': '+a', 'diff_truncated': false,
              'report': 'Patched.', 'errors': 0, 'tests': 'passed', 'rank': 2, 'judge_note': 'Fine',
              'usage': {'input_tokens': 1000, 'output_tokens': 200, 'cache_creation_tokens': 50, 'cached_tokens': 9000, 'estimate': false, 'runs': 1},
            },
            {
              'n': 2, 'hint': 'Refactor', 'status': 'failed', 'files': 0, 'insertions': 0, 'deletions': 0, 'diff': '', 'diff_truncated': false,
              'report': '', 'errors': 1, 'usage': {'input_tokens': 0, 'output_tokens': 0, 'cache_creation_tokens': 0, 'cached_tokens': 0, 'estimate': true, 'runs': 1},
            },
          ],
        },
      }) as ResponseBodyTaskVariants)
          .value;
      expect(view.phase, VariantPhase.ready);
      expect(view.judgePick, 2);
      expect(view.variants.first.changeLine, '2 files · +10 −3');
      expect(view.variants.first.tokens, 1250, reason: 'input, cache writes and output; cache reads not');
      expect(view.variants.first.choosable, isTrue);
      expect(view.variants.last.choosable, isFalse, reason: 'only a done trial can be chosen');

      final setting = (body({'type': 'variants_setting', 'value': {'enabled': true, 'max_variants': 3}}) as ResponseBodyVariantsSetting).value;
      expect(setting.enabled, isTrue);
      expect(setting.maxVariants, 3);

      final ready = (ServerMessage.fromJson({'type': 'event', 'seq': 7, 'kind': 'task.variants_ready', 'aggregate_id': 't', 'at_ms': 3}) as ServerMessageEvent).value;
      expect(ready.kind, EventKind.taskVariantsReady);
      expect(ready.isTask, isTrue);
    });

    test('a task with variants ready needs you; trying them reads as such', () {
      final ready = taskOf(status: TaskStatus.waiting, waitingReason: 'variants', variantPhase: VariantPhase.ready);
      expect(ready.needsYou, isTrue);
      expect(ready.variantsReady, isTrue);
      expect(ready.statusLabel, 'Variants ready to compare');
      final probing = taskOf(status: TaskStatus.running, variantPhase: VariantPhase.probing);
      expect(probing.needsYou, isFalse);
      expect(probing.statusLabel, 'Trying variants');
      expect(taskOf(status: TaskStatus.running).statusLabel, 'Running', reason: 'no variants, as before');
    });
  });

  test('receipts carry the result of the command', () {
    final receipt = (body({
      'type': 'receipt',
      'value': {
        'committed_seq': 91,
        'idempotency_key': 'k',
        'replayed': false,
        'result': {
          'kind': 'command',
          'value': {'type': 'chat_message_sent', 'conversation_id': 'c', 'message_id': 'm', 'run_id': 'r'},
        },
      },
    }) as ResponseBodyReceipt)
        .value;
    expect(receipt.idempotencyKey, 'k');
    expect(receipt.command, isA<CommandResultChatMessageSent>());
    expect((receipt.command as CommandResultChatMessageSent).conversationId, 'c');
  });

  test('errors, events and stream appends', () {
    final error = ServerMessage.fromJson({
      'type': 'response',
      'request_id': 7,
      'error': {'code': 'forbidden', 'message': 'This is only available in Aurelius on the PC.'},
    }) as ServerMessageResponse;
    expect(ProtocolException(error.error!.code, error.error!.message).isForbidden, isTrue);

    final event = (ServerMessage.fromJson({'type': 'event', 'seq': 92, 'kind': 'message.assistant_committed', 'aggregate_id': 'c', 'at_ms': 3}) as ServerMessageEvent).value;
    expect(event.kind, EventKind.messageAssistantCommitted);
    expect(event.isChat, isTrue);
    expect(event.isNoise, isFalse);
    final task = (ServerMessage.fromJson({'type': 'event', 'seq': 93, 'kind': 'task.created', 'aggregate_id': 't', 'project_id': 'p', 'at_ms': 3}) as ServerMessageEvent).value;
    expect(task.isChat, isFalse);
    expect(task.isTask, isTrue);

    // A deleted message that was not queued (AUR-FA): the same event, whatever its metadata says.
    final deleted = (ServerMessage.fromJson({
      'type': 'event',
      'seq': 94,
      'kind': 'message.deleted',
      'aggregate_id': 'c',
      'at_ms': 3,
      'metadata': {'message_id': 'm', 'by': 'u', 'queued': false},
    }) as ServerMessageEvent).value;
    expect(deleted.kind, EventKind.messageDeleted);
    expect(deleted.isChat, isTrue, reason: 'the chat reads the conversation again');
    expect(deleted.isNoise, isFalse);

    final append =(ServerMessage.fromJson({
      'type': 'stream_append',
      'run_id': 'r',
      'conversation_id': 'c',
      'message_id': 'm',
      'offset': 5,
      'text': ' there',
      'progress': {'started_at_ms': 1700000000000, 'output_tokens': 3900},
    }) as ServerMessageStreamAppend)
        .value;
    // "Hi 👋" is 5 UTF-16 units in Dart, as on the gateway.
    expect('Hi 👋'.length, 5);
    expect(append.applyTo('Hi 👋'), 'Hi 👋 there');
    expect(append.applyTo('Hi'), isNull, reason: 'a gap means refetch');
    expect(append.progress!.outputTokens, 3900);
    expect(append.progress!.startedAtMs, 1700000000000);
  });

  test('pairing QR payload', () {
    final p = PairingPayload.tryParse(
      '{"v":1,"url":"https://pc.tail1.ts.net:8443","code":"K7M29QXA","host_id":"h","host_name":"my-pc","env":"production"}',
    )!;
    expect(p.hostName, 'my-pc');
    expect(PairingPayload.tryParse('{"v":2,"url":"x","code":"y"}'), isNull);
    expect(PairingPayload.tryParse('hello'), isNull);
    final host = Host(hostId: 'h', name: 'pc', url: 'https://pc.tail1.ts.net:8443', clientId: 'c', environment: 'production', pairedAtMs: 0);
    expect(host.websocketUri.toString(), 'wss://pc.tail1.ts.net:8443/v1/ws');
  });

  test('ideas: who noted them and the task they became', () {
    final first = ideaOf('1', 'Dark mode', author: 'Iris', employeeId: 'm1');
    final last = ideaOf('2', 'Export', taskId: 't', taskStatus: TaskStatus.running);
    expect([first.byEmployee, last.byEmployee], [true, false]);
    expect(last.taskStatus, TaskStatus.running);
  });

  group('org chart', () {
    test('reads as a tree: bosses first, their reports indented, a missing boss makes a root', () {
      final chart = orgChart([
        employeeOf(id: 'dean', name: 'Dean', reportsTo: 'simon'),
        employeeOf(id: 'simon', name: 'Simon', reportsTo: 'jean'),
        employeeOf(id: 'jean', name: 'Jean'),
        employeeOf(id: 'max', name: 'Max', reportsTo: 'simon'),
        employeeOf(id: 'tom', name: 'Tom', reportsTo: 'archived-one'),
      ]);
      expect([for (final n in chart) (n.employee.name, n.depth)], [('Jean', 0), ('Simon', 1), ('Dean', 2), ('Max', 2), ('Tom', 0)]);
    });

    test('a cycle the server would refuse does not loop', () {
      final chart = orgChart([employeeOf(id: 'a', name: 'A', reportsTo: 'b'), employeeOf(id: 'b', name: 'B', reportsTo: 'a')]);
      expect(chart, isEmpty, reason: 'nobody is a root; nothing to show rather than looping');
    });

    test('a boss cannot be someone below you', () {
      final all = [
        employeeOf(id: 'jean', name: 'Jean'),
        employeeOf(id: 'simon', name: 'Simon', reportsTo: 'jean'),
        employeeOf(id: 'dean', name: 'Dean', reportsTo: 'simon'),
        employeeOf(id: 'tom', name: 'Tom'),
      ];
      expect(possibleBosses(all, all[1]).map((m) => m.name), ['Jean', 'Tom']);
    });
  });
}
