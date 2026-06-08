import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const root = path.join(__dirname, '..');
const contentDir = path.join(root, 'src/content/docs');

/** @type {Array<{id:string,title:string,phase:string,validationOf?:string,sections:Array<{slug:string,title:string,description:string,validates?:string,body:string}>}>} */
const chapters = [
  {
    id: 'preface',
    title: '前言',
    phase: 'meta',
    sections: [
      {
        slug: 'v-model-overview',
        title: 'V 模型概述',
        description: '介绍 V 模型左右两臂、底部实现与确认阶段的对应关系',
        body: sectionPrefaceVModel()
      },
      {
        slug: 'document-system',
        title: '文档体系与追溯',
        description: '说明各章交付物如何支撑定义—实现—确认闭环',
        body: sectionPrefaceDocSystem()
      },
      {
        slug: 'glossary',
        title: '术语与缩写',
        description: '统一 verification、validation、URD、规格书等术语',
        body: sectionPrefaceGlossary()
      },
      {
        slug: 'references',
        title: '引用标准',
        description: '参考 Wikipedia V 模型、IEEE、ISO 及行业规范',
        body: sectionPrefaceReferences()
      },
      {
        slug: 'roles',
        title: '角色与职责',
        description: '产品、架构、开发、测试、用户在 V 模型中的分工',
        body: sectionPrefaceRoles()
      }
    ]
  },
  {
    id: 'requirements',
    title: '需求分析',
    phase: 'definition',
    validationOf: 'acceptance',
    sections: [
      {
        slug: 'user-needs',
        title: '用户需要分析',
        description: '通过访谈与调研分析用户需要，明确理想系统目标',
        validates: 'acceptance/uat-execution',
        body: sectionRequirementsUserNeeds()
      },
      {
        slug: 'user-requirements-document',
        title: '用户需求文件',
        description: '编制用户需求文件（URD），作为验收测试的源头依据',
        validates: 'acceptance/acceptance-criteria',
        body: sectionRequirementsURD()
      },
      {
        slug: 'functional-requirements',
        title: '功能需求整理',
        description: '整理可验证的系统功能需求，不约束具体实现方式',
        validates: 'acceptance/scenario-validation',
        body: sectionRequirementsFunctional()
      },
      {
        slug: 'uat-plan',
        title: '用户验收测试计划',
        description: '在需求阶段早期制定 UAT 计划，与需求条目一一对应',
        validates: 'acceptance/uat-execution',
        body: sectionRequirementsUATPlan()
      },
      {
        slug: 'requirements-baseline',
        title: '需求评审与基线',
        description: '评审、签发需求基线，建立向下游设计与测试的追溯',
        body: sectionRequirementsBaseline()
      }
    ]
  },
  {
    id: 'system-design',
    title: '系统设计',
    phase: 'definition',
    validationOf: 'system-test',
    sections: [
      {
        slug: 'software-specification',
        title: '软件规格说明书',
        description: '输出开发蓝图：架构、菜单、数据结构等高层设计',
        validates: 'system-test/system-test-execution',
        body: sectionSystemDesignSpec()
      },
      {
        slug: 'architecture-and-ui',
        title: '系统架构与界面结构',
        description: '描述大致系统架构、指令菜单结构与主要交互',
        validates: 'system-test/functional-verification',
        body: sectionSystemDesignArchUI()
      },
      {
        slug: 'business-scenarios',
        title: '业务场景与样例',
        description: '业务场景、样例视窗与报表，帮助理解系统行为',
        validates: 'system-test/scenario-coverage',
        body: sectionSystemDesignScenarios()
      },
      {
        slug: 'entity-data-dictionary',
        title: '实体图与数据字典',
        description: '实体图、数据字典等技术文件，支撑数据一致性验证',
        validates: 'system-test/data-validation',
        body: sectionSystemDesignEntity()
      },
      {
        slug: 'system-test-plan',
        title: '系统测试计划',
        description: '在系统设计阶段规划系统测试范围、环境与通过准则',
        validates: 'system-test/system-test-execution',
        body: sectionSystemDesignTestPlan()
      }
    ]
  },
  {
    id: 'architecture',
    title: '架构设计',
    phase: 'definition',
    validationOf: 'integration-test',
    sections: [
      {
        slug: 'architecture-views',
        title: '架构视图与边界',
        description: '上下文、容器、组件视图，明确子系统边界',
        validates: 'integration-test/integration-scope',
        body: sectionArchitectureViews()
      },
      {
        slug: 'subsystem-decomposition',
        title: '子系统划分',
        description: '将系统拆分为可集成的子系统/服务单元',
        validates: 'integration-test/component-integration',
        body: sectionArchitectureSubsystem()
      },
      {
        slug: 'integration-strategy',
        title: '集成策略',
        description: '大爆炸、自顶向下、自底向上或持续集成策略',
        validates: 'integration-test/integration-approach',
        body: sectionArchitectureIntegrationStrategy()
      },
      {
        slug: 'interface-contracts',
        title: '接口与通信契约',
        description: '子系统间协议、消息格式、时序与错误约定',
        validates: 'integration-test/interface-testing',
        body: sectionArchitectureInterfaces()
      },
      {
        slug: 'integration-test-plan',
        title: '集成测试计划',
        description: '在架构阶段规划集成测试项与通过标准',
        validates: 'integration-test/integration-test-execution',
        body: sectionArchitectureTestPlan()
      }
    ]
  },
  {
    id: 'modules',
    title: '模组设计',
    phase: 'definition',
    validationOf: 'unit-test',
    sections: [
      {
        slug: 'module-decomposition',
        title: '模组拆解',
        description: '低阶设计：将系统拆解为可编码的最小交付单元',
        validates: 'unit-test/unit-scope',
        body: sectionModulesDecomposition()
      },
      {
        slug: 'logic-pseudocode',
        title: '逻辑细节与伪代码',
        description: '模组内部逻辑、算法与伪代码，指导程序设计',
        validates: 'unit-test/logic-verification',
        body: sectionModulesLogic()
      },
      {
        slug: 'database-tables',
        title: '数据库表设计',
        description: '表结构、字段型别、长度、约束与索引',
        validates: 'unit-test/data-layer-testing',
        body: sectionModulesDatabase()
      },
      {
        slug: 'api-dependencies-io',
        title: '接口、依赖与输入输出',
        description: 'API 细节、相依性、错误消息、模组 I/O 列表',
        validates: 'unit-test/interface-unit-testing',
        body: sectionModulesAPI()
      },
      {
        slug: 'unit-test-plan',
        title: '单元测试计划',
        description: '在模组设计阶段规划单元测试，消除代码/单元级错误',
        validates: 'unit-test/unit-test-execution',
        body: sectionModulesUnitTestPlan()
      }
    ]
  },
  {
    id: 'implementation',
    title: '代码实现',
    phase: 'implementation',
    sections: [
      {
        slug: 'implementation-baseline',
        title: '实现依据与顺序',
        description: '依据模组设计/程序规格书开展编码与集成顺序',
        body: sectionImplementationBaseline()
      },
      {
        slug: 'coding-standards',
        title: '编码规范',
        description: '命名、格式、异常处理、日志与可测试性约定',
        body: sectionImplementationCoding()
      },
      {
        slug: 'build-ci',
        title: '构建与持续集成',
        description: '编译、打包、静态检查与 CI 流水线',
        body: sectionImplementationCI()
      },
      {
        slug: 'configuration',
        title: '配置与密钥管理',
        description: '环境配置、特性开关与密钥安全存储',
        body: sectionImplementationConfig()
      },
      {
        slug: 'code-review',
        title: '代码审查',
        description: '对照低阶设计进行审查，作为单元测试前门禁',
        body: sectionImplementationReview()
      }
    ]
  },
  {
    id: 'unit-test',
    title: '单元测试',
    phase: 'validation',
    sections: [
      {
        slug: 'unit-test-execution',
        title: '单元测试计划执行',
        description: '执行模组设计阶段制定的单元测试计划',
        body: sectionUnitTestExecution()
      },
      {
        slug: 'isolation-testing',
        title: '隔离测试',
        description: '验证最小程序体在与其他程序隔离时能否正常运作',
        body: sectionUnitTestIsolation()
      },
      {
        slug: 'test-cases',
        title: '测试用例设计与执行',
        description: '覆盖正常、边界、异常路径，关联模组 I/O',
        body: sectionUnitTestCases()
      },
      {
        slug: 'defect-handling',
        title: '缺陷处理',
        description: '记录、分级、修复与回归验证单元级缺陷',
        body: sectionUnitTestDefects()
      },
      {
        slug: 'unit-test-report',
        title: '单元测试报告',
        description: '通过率、覆盖率、遗留缺陷与对集成测试的输入',
        body: sectionUnitTestReport()
      }
    ]
  },
  {
    id: 'integration-test',
    title: '集成测试',
    phase: 'validation',
    sections: [
      {
        slug: 'integration-test-execution',
        title: '集成测试计划执行',
        description: '执行架构设计阶段制定的集成测试计划',
        body: sectionIntegrationExecution()
      },
      {
        slug: 'interface-testing',
        title: '接口联调验证',
        description: '验证子系统/模组间接口契约与时序',
        body: sectionIntegrationInterface()
      },
      {
        slug: 'integration-scenarios',
        title: '集成场景测试',
        description: '跨组件业务流程、事务与一致性场景',
        body: sectionIntegrationScenarios()
      },
      {
        slug: 'defect-handling',
        title: '缺陷处理',
        description: '集成缺陷定位、修复与回归策略',
        body: sectionIntegrationDefects()
      },
      {
        slug: 'integration-test-report',
        title: '集成测试报告',
        description: '集成结论、风险项与系统测试准入评估',
        body: sectionIntegrationReport()
      }
    ]
  },
  {
    id: 'system-test',
    title: '系统测试',
    phase: 'validation',
    sections: [
      {
        slug: 'system-test-execution',
        title: '系统测试计划执行',
        description: '执行系统设计阶段制定的系统测试计划',
        body: sectionSystemTestExecution()
      },
      {
        slug: 'functional-verification',
        title: '功能验证',
        description: '端到端验证软件规格说明书中的功能与界面行为',
        body: sectionSystemTestFunctional()
      },
      {
        slug: 'nonfunctional-verification',
        title: '非功能验证',
        description: '性能、安全、可用性、兼容性等质量属性',
        body: sectionSystemTestNonfunctional()
      },
      {
        slug: 'defect-handling',
        title: '缺陷处理',
        description: '系统级缺陷闭环与发布风险评估',
        body: sectionSystemTestDefects()
      },
      {
        slug: 'system-test-report',
        title: '系统测试报告',
        description: '系统测试结论与验收测试准入建议',
        body: sectionSystemTestReport()
      }
    ]
  },
  {
    id: 'acceptance',
    title: '验收测试',
    phase: 'validation',
    sections: [
      {
        slug: 'uat-execution',
        title: '用户验收测试执行',
        description: '由企业用户执行需求阶段制定的 UAT 计划',
        body: sectionAcceptanceUAT()
      },
      {
        slug: 'user-environment',
        title: '用户环境验证',
        description: '在用户环境下模拟实际产品运行条件',
        body: sectionAcceptanceEnvironment()
      },
      {
        slug: 'real-data-validation',
        title: '真实数据验证',
        description: '使用实际数据进行验收，确认可投产使用',
        body: sectionAcceptanceRealData()
      },
      {
        slug: 'acceptance-criteria',
        title: '验收结论与签字',
        description: '确认系统符合客户需求并满足验收标准',
        body: sectionAcceptanceCriteria()
      },
      {
        slug: 'release-handover',
        title: '投产与移交',
        description: '上线步骤、运维移交与交付物清单',
        body: sectionAcceptanceRelease()
      }
    ]
  },
  {
    id: 'appendix',
    title: '附录',
    phase: 'meta',
    sections: [
      {
        slug: 'v-model-mapping',
        title: 'V 模型对应关系表',
        description: '定义阶段交付物与确认阶段测试的完整映射',
        body: sectionAppendixMapping()
      },
      {
        slug: 'templates',
        title: '文档模板清单',
        description: 'URD、规格书、低阶设计、各阶段测试计划模板',
        body: sectionAppendixTemplates()
      },
      {
        slug: 'changelog',
        title: '变更记录',
        description: '基线变更历史与影响分析',
        body: sectionAppendixChangelog()
      },
      {
        slug: 'bibliography',
        title: '参考资料',
        description: 'Wikipedia V 模型、Pressman、Beizer 等文献',
        body: sectionAppendixBibliography()
      },
      {
        slug: 'approval',
        title: '审批记录',
        description: '各阶段评审与签发留痕',
        body: sectionAppendixApproval()
      }
    ]
  }
];

// ── Article body generators ─────────────────────────────────────

function vModelHeader(chapterTitle, sectionTitle, phase, pair) {
  const pairLine = pair
    ? `\n> **V 模型对应**：本章属于项目定义阶段（左侧），与确认阶段 **${pair}** 一一对应。测试计划应在设计阶段早期制定，而非实现后才补写。`
    : phase === 'implementation'
      ? '\n> **V 模型对应**：位于 V 模型底部，将左侧低阶设计转化为可执行代码。'
      : phase === 'validation'
        ? '\n> **V 模型对应**：属于确认阶段（右侧），验证左侧对应设计阶段的交付物。'
        : '';
  return `## 阶段定位

本文属于 **${chapterTitle}** 章节的 **${sectionTitle}** 小节。${pairLine}

参考：[V 模型（软件开发）](https://zh.wikipedia.org/wiki/V%E6%A8%A1%E5%9E%8B_(%E8%BB%9F%E9%AB%94%E9%96%8B%E7%99%BC))`;
}

function deliverablesTable(rows) {
  return `## 交付物

| 交付物 | 说明 | 责任人 |
| --- | --- | --- |
${rows.map(([a, b, c]) => `| ${a} | ${b} | ${c} |`).join('\n')}`;
}

function checklist(items) {
  return `## 检查清单\n\n${items.map((i) => `- [ ] ${i}`).join('\n')}`;
}

function cabPlaceholder() {
  return `## CAB 项目待填

> 以下为 CAB 项目落地时填写。

- 具体内容：
- 责任人 / 评审人：
- 关联需求/设计编号：
- 附件与链接：`;
}

// Preface
function sectionPrefaceVModel() {
  return `${vModelHeader('前言', 'V 模型概述', 'meta', '')}

## V 模型结构

V 模型将软件生命周期分为：

1. **项目定义阶段（左侧 / Verification）**：需求分析 → 系统设计 → 架构设计 → 模组设计
2. **实现阶段（底部）**：代码实现
3. **确认阶段（右侧 / Validation）**：单元测试 → 集成测试 → 系统测试 → 验收测试

左侧每一阶段都应在相应设计时点**同步规划**右侧测试，形成「设计即规划测试」的闭环。

\`\`\`
需求分析 ──────────────── 验收测试
    ╲                         ╱
系统设计 ────────────── 系统测试
      ╲                   ╱
架构设计 ────────── 集成测试
        ╲             ╱
模组设计 ────── 单元测试
          ╲     ╱
        代码实现
\`\`\`

## 本文档体系原则

- **早期测试计划**：验收测试计划在需求分析阶段制定；系统/集成/单元测试计划分别在系统、架构、模组设计阶段制定。
- **需求不绑定实现**：需求分析描述「理想系统」，不规定具体软件设计方式。
- **低阶设计可编码**：模组设计须细到程序设计者可依文档直接编码。
- **隔离验证**：单元测试验证最小程序体在隔离环境下能否正常运作。

${checklist([
  '团队理解 V 模型左右对应关系',
  '明确各章在定义/实现/确认中的位置',
  '确认测试计划编制时点不滞后于设计阶段'
])}

${cabPlaceholder()}`;
}

function sectionPrefaceDocSystem() {
  return `${vModelHeader('前言', '文档体系与追溯', 'meta', '')}

## 追溯规则

| 定义阶段文档 | 确认阶段验证 | 追溯方式 |
| --- | --- | --- |
| 用户需求文件（URD） | 用户验收测试 | 需求 ID ↔ UAT 用例 |
| 软件规格说明书 | 系统测试计划/执行 | 功能条目 ↔ 系统测试用例 |
| 架构/集成设计 | 集成测试计划/执行 | 接口契约 ↔ 集成用例 |
| 低阶设计/程序规格书 | 单元测试计划/执行 | 模组 I/O ↔ 单元用例 |

## 编号建议

- 需求：\`REQ-###\`
- 设计：\`DES-###\` / \`ARCH-###\` / \`MOD-###\`
- 测试：\`UT-###\` / \`IT-###\` / \`ST-###\` / \`UAT-###\`

${checklist([
  '建立双向追溯矩阵',
  '变更时同步更新对侧测试文档',
  '基线化后变更走变更记录流程（见附录）'
])}

${cabPlaceholder()}`;
}

function sectionPrefaceGlossary() {
  return `${vModelHeader('前言', '术语与缩写', 'meta', '')}

## 核心术语

| 术语 | 定义 |
| --- | --- |
| Verification（验证） | 项目定义阶段：「是否按规格正确地构建」 |
| Validation（确认） | 确认阶段：「是否构建了正确的系统」 |
| URD | User Requirements Document，用户需求文件 |
| 低阶设计 | 模组设计，含伪代码、表结构、API 细节 |
| 单元 | 可独立存在的最小程序体，如模块、函数集合 |

## 缩写

| 缩写 | 全称 |
| --- | --- |
| UAT | User Acceptance Test，用户验收测试 |
| STP | System Test Plan，系统测试计划 |
| ITP | Integration Test Plan，集成测试计划 |
| UTP | Unit Test Plan，单元测试计划 |

${cabPlaceholder()}`;
}

function sectionPrefaceReferences() {
  return `${vModelHeader('前言', '引用标准', 'meta', '')}

## 主要参考

- [V 模型（软件开发）— 维基百科](https://zh.wikipedia.org/wiki/V%E6%A8%A1%E5%9E%8B_(%E8%BB%9F%E9%AB%94%E9%96%8B%E7%99%BC))
- Roger S. Pressman: *Software Engineering: A Practitioner's Approach*
- Boris Beizer: *Software Testing Techniques*

## 可选标准

- IEEE 830（需求规格）
- ISO/IEC/IEEE 29119（软件测试）

${cabPlaceholder()}`;
}

function sectionPrefaceRoles() {
  return `${vModelHeader('前言', '角色与职责', 'meta', '')}

| 角色 | 定义阶段 | 确认阶段 |
| --- | --- | --- |
| 产品/业务分析 | 需求分析、URD、UAT 计划 | 组织 UAT、验收签字 |
| 系统分析/设计 | 系统设计、规格书、系统测试计划 | 支持系统测试评审 |
| 架构师 | 架构设计、集成测试计划 | 支持集成测试 |
| 开发/模组设计 | 模组设计、单元测试计划、编码 | 修复缺陷、单元测试 |
| 测试工程师 | 协助各阶段测试计划与执行 | 执行 UT/IT/ST |
| 用户代表 | 提供需求与验收场景 | 执行 UAT |

${cabPlaceholder()}`;
}

// Requirements
function sectionRequirementsUserNeeds() {
  return `${vModelHeader('需求分析', '用户需要分析', 'definition', '验收测试')}

## 目标

分析用户的真实需要，建构**理想系统**愿景。此阶段**不必**决定软件的具体设计方式。

## 活动

1. 与目标用户面谈、问卷或现场观察
2. 识别干系人、使用场景与痛点
3. 区分「需要」与「想要」，记录约束（法规、预算、时间）
4. 输出用户需要摘要，作为 URD 输入

${deliverablesTable([
  ['用户需要调研记录', '访谈纪要、问卷统计', '产品/BA'],
  ['干系人清单', '角色、利益、影响力', '产品/BA'],
  ['场景草图', '高层业务流程（非界面细节）', '产品/BA']
])}

${checklist([
  '覆盖主要用户角色',
  '需要表述可测试、可观察',
  '已识别验收测试关键场景来源'
])}

${cabPlaceholder()}`;
}

function sectionRequirementsURD() {
  return `${vModelHeader('需求分析', '用户需求文件', 'definition', '验收测试')}

## 目标

编制**用户需求文件（URD）**，作为整个 V 模型右侧验收测试的**源头文档**。

## URD 必备内容

1. 文档标识与版本
2. 项目背景与目标
3. 用户角色与权限期望
4. 业务场景描述（Given-When-Then 或等价形式）
5. 功能需求条目（可编号、可验证）
6. 非功能需求（性能、安全、可用性）
7. 约束与假设
8. 验收准则索引（指向 UAT 计划条目）

## 编写规范

- 使用用户语言，避免实现术语（如具体框架、数据库表名）
- 每条需求应**可验证**，便于导出 UAT 用例
- 与《用户验收测试计划》保持双向引用

${checklist([
  '每条需求有唯一 ID',
  '需求无歧义、无矛盾',
  'URD 经评审并基线化'
])}

${cabPlaceholder()}`;
}

function sectionRequirementsFunctional() {
  return `${vModelHeader('需求分析', '功能需求整理', 'definition', '验收测试')}

## 目标

将用户需要整理为结构化的**系统功能需求**，明确系统应做什么。

## 内容要求

| 字段 | 说明 |
| --- | --- |
| 需求 ID | 如 REQ-F-001 |
| 标题 | 简短动词短语 |
| 描述 | 系统行为说明 |
| 优先级 | Must / Should / Could |
| 验收条件 | 可度量、可演示的通过标准 |
| 来源 | 访谈/法规/竞品等 |

## 与设计的边界

功能需求**不得**规定：具体 API 路径、类名、数据库 schema、框架选型——这些属于后续系统设计/模组设计。

${checklist([
  '功能需求覆盖 URD 全部场景',
  '验收条件可直接映射 UAT 用例',
  '非功能需求已单独列出'
])}

${cabPlaceholder()}`;
}

function sectionRequirementsUATPlan() {
  return `${vModelHeader('需求分析', '用户验收测试计划', 'definition', '验收测试')}

## 目标

依维基百科 V 模型：**用户验收测试计划应在需求分析阶段就订定**，由企业用户主导规划。

## 计划必备章节

1. 测试范围与不在范围
2. 参与角色（用户代表、产品、测试）
3. 环境要求（拟真生产环境）
4. 数据策略（**实际数据**或脱敏生产数据）
5. 用例列表：UAT ID ↔ 需求 ID
6. 通过/失败准则与签字流程
7. 日程与资源

## 原则

- UAT 验证「系统是否符合客户需求且可在实际环境使用」
- 用例来源于 URD/功能需求，不得凭空增补未基线需求

${checklist([
  '每个 Must 级需求至少一条 UAT 用例',
  '用户代表已评审计划',
  '与 URD 追溯矩阵已建立'
])}

${cabPlaceholder()}`;
}

function sectionRequirementsBaseline() {
  return `${vModelHeader('需求分析', '需求评审与基线', 'definition', '验收测试')}

## 目标

通过正式评审将 URD、功能需求、UAT 计划**基线化**，作为下游设计的唯一输入。

## 评审要点

- 完整性、一致性、可测试性、可行性
- 与业务目标对齐
- UAT 计划能否覆盖关键风险

## 基线后变更

任何需求变更须：变更申请 → 影响分析（设计/测试） → 更新追溯矩阵 → 重新签发（见附录变更记录）

${checklist([
  '评审会议纪要已归档',
  '基线版本号已标记',
  '下游设计负责人已签收'
])}

${cabPlaceholder()}`;
}

// System design
function sectionSystemDesignSpec() {
  return `${vModelHeader('系统设计', '软件规格说明书', 'definition', '系统测试')}

## 目标

产出**软件规格说明书（Software Specification Document）**，作为开发阶段的蓝图。

## 必备内容（依 V 模型）

1. 系统概述与范围
2. 大致**系统架构**
3. **指令/菜单结构**
4. **数据结构**高层说明
5. 主要处理流程
6. 外部系统接口概述
7. 与系统测试计划的交叉引用

## 编写规范

- 面向系统分析与开发团队，可含界面与报表说明
- 仍属高层设计，具体模组逻辑放在模组设计章

${checklist([
  '规格书覆盖全部基线功能需求',
  '已引用实体图与数据字典',
  '系统测试计划已同步起草'
])}

${cabPlaceholder()}`;
}

function sectionSystemDesignArchUI() {
  return `${vModelHeader('系统设计', '系统架构与界面结构', 'definition', '系统测试')}

## 目标

描述系统架构与**指令菜单结构**，使读者理解系统如何组织与呈现。

## 内容

- 逻辑分层（展示层、业务层、数据层等）
- 主要模块职责概览
- 菜单/导航/命令结构
- 关键界面线框或说明（非高保真也可）
- 与架构设计章的衔接点

${checklist([
  '架构图与菜单树一致',
  '界面结构覆盖主要用户任务',
  '系统测试可据此设计界面遍历用例'
])}

${cabPlaceholder()}`;
}

function sectionSystemDesignScenarios() {
  return `${vModelHeader('系统设计', '业务场景与样例', 'definition', '系统测试')}

## 目标

通过**业务场景、样例视窗、报表**帮助理解系统行为，降低误解。

## 内容格式

每个场景建议包含：

| 项 | 说明 |
| --- | --- |
| 场景 ID | SC-### |
| 触发条件 | 用户动作或系统事件 |
| 主成功路径 | 步骤序列 |
| 扩展/异常 | 分支说明 |
| 样例界面 | 截图或草图引用 |
| 样例报表 | 输出样例 |

${checklist([
  '场景与 URD 需求可追溯',
  '覆盖关键异常与边界',
  '系统测试场景用例可据此导出'
])}

${cabPlaceholder()}`;
}

function sectionSystemDesignEntity() {
  return `${vModelHeader('系统设计', '实体图与数据字典', 'definition', '系统测试')}

## 目标

编制**实体图（Entity Diagram）**与**数据字典**，为数据设计与系统测试提供依据。

## 数据字典字段

| 字段 | 说明 |
| --- | --- |
| 实体名 | 业务对象 |
| 属性名 | 字段名 |
| 类型/长度 | 逻辑类型 |
| 必填 | 是/否 |
| 业务规则 | 校验、默认值 |
| 来源/去向 | 上游下游 |

## 与下游关系

- 模组设计章将细化为物理表结构
- 系统测试须验证数据完整性、一致性

${checklist([
  '实体与需求领域名词一致',
  '数据字典无遗漏关键属性',
  '系统测试含数据校验用例'
])}

${cabPlaceholder()}`;
}

function sectionSystemDesignTestPlan() {
  return `${vModelHeader('系统设计', '系统测试计划', 'definition', '系统测试')}

## 目标

在**系统设计阶段**整理**系统测试**文档，而非等到编码完成才规划。

## 计划内容

1. 测试范围（对照软件规格说明书）
2. 测试环境（类生产）
3. 功能测试策略
4. 非功能测试策略（性能、安全等）
5. 用例结构：ST ID ↔ 需求/场景 ID
6. 入口/出口准则
7. 角色与日程

${checklist([
  '计划覆盖规格书全部章节',
  '与验收测试边界清晰（ST vs UAT）',
  '系统测试执行章可直接沿用本计划'
])}

${cabPlaceholder()}`;
}

// Architecture - abbreviated implementations for remaining sections
function sectionArchitectureViews() {
  return `${vModelHeader('架构设计', '架构视图与边界', 'definition', '集成测试')}

## 目标

定义系统**上下文、容器、组件**视图，明确子系统边界与外部依赖。

## 内容

- 上下文图：系统与用户、外部系统关系
- 容器图：进程/服务/应用边界
- 组件图：主要技术组件
- 部署相关约束（网络、端口、协议）

${checklist(['边界与软件规格说明书一致', '外部接口已编号', '集成测试范围可据此划定'])}

${cabPlaceholder()}`;
}

function sectionArchitectureSubsystem() {
  return `${vModelHeader('架构设计', '子系统划分', 'definition', '集成测试')}

## 目标

将系统拆分为可独立集成与测试的**子系统/服务**。

## 划分原则

- 高内聚、低耦合
- 明确 Owner 团队
- 可独立部署或编译单元（视架构风格而定）

${deliverablesTable([
  ['子系统清单', '名称、职责、依赖', '架构师'],
  ['依赖矩阵', '子系统间调用关系', '架构师']
])}

${cabPlaceholder()}`;
}

function sectionArchitectureIntegrationStrategy() {
  return `${vModelHeader('架构设计', '集成策略', 'definition', '集成测试')}

## 目标

选择并文档化集成策略：大爆炸、自顶向下、自底向上、**持续集成**等。

## 内容

- 集成顺序与里程碑
- 桩/驱动器需求
- CI 流水线中的集成测试挂载点
- 失败回滚策略

${cabPlaceholder()}`;
}

function sectionArchitectureInterfaces() {
  return `${vModelHeader('架构设计', '接口与通信契约', 'definition', '集成测试')}

## 目标

定义子系统间**接口契约**，作为集成测试的直接依据。

## 契约内容

- 协议与端点
- 请求/响应 schema
- 错误码与重试语义
- 时序图（关键流程）
- 版本与兼容策略

${checklist(['每个跨子系统接口有唯一 ID', '集成测试用例可映射到接口 ID'])}

${cabPlaceholder()}`;
}

function sectionArchitectureTestPlan() {
  return `${vModelHeader('架构设计', '集成测试计划', 'definition', '集成测试')}

## 目标

在架构设计阶段制定**集成测试计划**，对应架构与子系统接口。

## 计划内容

1. 集成范围（子系统对）
2. 策略与顺序
3. 环境、桩、测试数据
4. 用例：IT ID ↔ 接口/架构元素
5. 通过准则

${cabPlaceholder()}`;
}

// Modules
function sectionModulesDecomposition() {
  return `${vModelHeader('模组设计', '模组拆解', 'definition', '单元测试')}

## 目标

**低阶设计**：将设计拆解为较小单元/模组，说明每部分职责，使程序设计者可依文档编码。

## 内容

- 模组清单与职责
- 模组层次图
- 与架构子系统的映射
- 编码顺序建议

${cabPlaceholder()}`;
}

function sectionModulesLogic() {
  return `${vModelHeader('模组设计', '逻辑细节与伪代码', 'definition', '单元测试')}

## 目标

描述模组**逻辑细节**，可以**伪代码**表示，指导实现。

## 必备内容

- 主要算法与决策分支
- 前置/后置条件
- 异常处理逻辑
- 伪代码或活动图

${cabPlaceholder()}`;
}

function sectionModulesDatabase() {
  return `${vModelHeader('模组设计', '数据库表设计', 'definition', '单元测试')}

## 目标

依 V 模型，低阶设计须包含**数据库表**：所有元素、型别、大小。

## 表设计模板

| 列名 | 类型 | 长度 | 空 | 默认 | 说明 |
| --- | --- | --- | --- | --- | --- |
| id | UUID | - | N | - | 主键 |

含索引、外键、约束、迁移说明。

${cabPlaceholder()}`;
}

function sectionModulesAPI() {
  return `${vModelHeader('模组设计', '接口、依赖与输入输出', 'definition', '单元测试')}

## 目标

完整描述模组级**应用程序接口**与**相依性**。

## 低阶设计必备（维基百科）

- 完整 API 接口细节
- 所有相依性议题
- **错误消息列表**
- 模组所有**输入及输出**

${checklist([
  '每个公开函数/接口有 I/O 表',
  '错误码与消息成对列出',
  '单元测试可覆盖每个 I/O 与错误分支'
])}

${cabPlaceholder()}`;
}

function sectionModulesUnitTestPlan() {
  return `${vModelHeader('模组设计', '单元测试计划', 'definition', '单元测试')}

## 目标

依 V 模型，在**模组设计阶段**规划**单元测试计划（UTP）**，目的是消除**程序码层级及单元层级**的错误。

## 计划内容

1. 测试范围（模组列表）
2. 隔离策略与 Mock 边界
3. 用例：UT ID ↔ 模组 I/O / 逻辑分支
4. 覆盖率目标
5. 工具链与 CI 挂载

## 原则

**单元**是程序中可独立存在的最小程序体；测试须验证其在**隔离**环境下能否正常运作。

${cabPlaceholder()}`;
}

// Implementation
function sectionImplementationBaseline() {
  return `${vModelHeader('代码实现', '实现依据与顺序', 'implementation', '')}

## 目标

依据**程序规格书/低阶设计**开展编码，明确实现顺序与完成定义。

## 内容

- 引用基线：MOD-###、UTP 清单
- 实现顺序（依赖拓扑）
- 完成定义（DoD）：代码、单测、审查、文档

${cabPlaceholder()}`;
}

function sectionImplementationCoding() {
  return `${vModelHeader('代码实现', '编码规范', 'implementation', '')}

## 目标

统一编码风格，保证可维护性与可测试性。

## 规范领域

- 命名、目录、格式
- 错误处理与日志
- 并发与资源管理
- 安全编码（输入校验、密钥）

${cabPlaceholder()}`;
}

function sectionImplementationCI() {
  return `${vModelHeader('代码实现', '构建与持续集成', 'implementation', '')}

## 目标

建立可重复的构建与 CI，在合并前运行单元测试。

## 内容

- 构建命令与产物
- CI 阶段：lint → build → unit test
- 失败通知与修复 SLA

${cabPlaceholder()}`;
}

function sectionImplementationConfig() {
  return `${vModelHeader('代码实现', '配置与密钥管理', 'implementation', '')}

## 目标

管理环境配置与密钥，避免硬编码敏感信息。

${cabPlaceholder()}`;
}

function sectionImplementationReview() {
  return `${vModelHeader('代码实现', '代码审查', 'implementation', '')}

## 目标

对照低阶设计审查实现，作为进入单元测试正式执行前的质量门禁。

## 审查清单

- 实现与伪代码/接口说明一致
- 错误处理覆盖错误消息列表
- 可测试性（依赖可注入）

${cabPlaceholder()}`;
}

// Unit test
function sectionUnitTestExecution() {
  return `${vModelHeader('单元测试', '单元测试计划执行', 'validation', '')}

## 目标

**执行**模组设计阶段已制定的单元测试计划。

## 活动

1. 搭建隔离测试环境
2. 按 UTP 执行用例
3. 记录结果与覆盖率
4. 未通过用例进入缺陷流程

${checklist(['UTP 版本与模组设计基线一致', '所有计划用例至少执行一次'])}

${cabPlaceholder()}`;
}

function sectionUnitTestIsolation() {
  return `${vModelHeader('单元测试', '隔离测试', 'validation', '')}

## 目标

验证**最小程序体**在与其他程序**隔离**时能否正常运作（V 模型原义）。

## 实践

- 使用 Mock/Stub 切断外部 DB、网络、文件系统
- 每个测试独立、可重复、确定性
- 避免集成测试渗入单元测试

${cabPlaceholder()}`;
}

function sectionUnitTestCases() {
  return `${vModelHeader('单元测试', '测试用例设计与执行', 'validation', '')}

## 目标

用例覆盖正常、边界、异常路径，并关联模组 I/O 表。

## 用例字段

| 字段 | 说明 |
| --- | --- |
| UT ID | 用例编号 |
| 模组 | 被测单元 |
| 输入 | 来自 I/O 定义 |
| 期望输出 | 含错误消息 |
| 追溯 | MOD / 逻辑分支 ID |

${cabPlaceholder()}`;
}

function sectionUnitTestDefects() {
  return `${vModelHeader('单元测试', '缺陷处理', 'validation', '')}

## 目标

单元级缺陷的记录、分级、修复、回归。

## 分级建议

| 级别 | 说明 |
| --- | --- |
| Blocker | 核心逻辑错误，阻塞集成 |
| Major | 功能错误 |
| Minor | 边界/文案等 |

${cabPlaceholder()}`;
}

function sectionUnitTestReport() {
  return `${vModelHeader('单元测试', '单元测试报告', 'validation', '')}

## 目标

总结单元测试结论，作为**集成测试准入**输入。

## 报告内容

- 执行摘要、通过率、覆盖率
- 遗留缺陷与风险
- 对集成测试的建议（模组稳定性）

${cabPlaceholder()}`;
}

// Integration test
function sectionIntegrationExecution() {
  return `${vModelHeader('集成测试', '集成测试计划执行', 'validation', '')}

## 目标

执行架构设计阶段制定的**集成测试计划**。

${checklist(['集成顺序与架构设计一致', '接口契约已全部覆盖'])}

${cabPlaceholder()}`;
}

function sectionIntegrationInterface() {
  return `${vModelHeader('集成测试', '接口联调验证', 'validation', '')}

## 目标

验证子系统/模组间**接口契约**：协议、schema、错误、时序。

${cabPlaceholder()}`;
}

function sectionIntegrationScenarios() {
  return `${vModelHeader('集成测试', '集成场景测试', 'validation', '')}

## 目标

跨组件业务流程、分布式事务、幂等与一致性场景。

${cabPlaceholder()}`;
}

function sectionIntegrationDefects() {
  return `${vModelHeader('集成测试', '缺陷处理', 'validation', '')}

## 目标

集成缺陷通常涉及多方模组，须记录调用链与接口 ID 便于定位。

${cabPlaceholder()}`;
}

function sectionIntegrationReport() {
  return `${vModelHeader('集成测试', '集成测试报告', 'validation', '')}

## 目标

集成结论与**系统测试准入**评估。

${cabPlaceholder()}`;
}

// System test
function sectionSystemTestExecution() {
  return `${vModelHeader('系统测试', '系统测试计划执行', 'validation', '')}

## 目标

执行系统设计阶段制定的**系统测试计划**。

${cabPlaceholder()}`;
}

function sectionSystemTestFunctional() {
  return `${vModelHeader('系统测试', '功能验证', 'validation', '')}

## 目标

端到端验证**软件规格说明书**中的功能、菜单、场景与报表。

${cabPlaceholder()}`;
}

function sectionSystemTestNonfunctional() {
  return `${vModelHeader('系统测试', '非功能验证', 'validation', '')}

## 目标

性能、安全、可用性、兼容性等（若在需求/规格中定义）。

${cabPlaceholder()}`;
}

function sectionSystemTestDefects() {
  return `${vModelHeader('系统测试', '缺陷处理', 'validation', '')}

## 目标

系统级缺陷闭环，评估发布风险。

${cabPlaceholder()}`;
}

function sectionSystemTestReport() {
  return `${vModelHeader('系统测试', '系统测试报告', 'validation', '')}

## 目标

系统测试结论与**验收测试（UAT）准入**建议。

${cabPlaceholder()}`;
}

// Acceptance
function sectionAcceptanceUAT() {
  return `${vModelHeader('验收测试', '用户验收测试执行', 'validation', '')}

## 目标

由**企业用户**执行需求分析阶段制定的 **UAT 计划**。

## 原则（V 模型）

- 测试计划早在需求阶段已订定
- 用户在**自己的环境**下执行
- 验证系统是否符合客户需求

${cabPlaceholder()}`;
}

function sectionAcceptanceEnvironment() {
  return `${vModelHeader('验收测试', '用户环境验证', 'validation', '')}

## 目标

在**用户环境**下设法**模拟实际产品环境**，而非仅开发/测试环境。

## 内容

- 硬件、网络、权限、第三方系统
- 与生产差异说明与风险接受

${cabPlaceholder()}`;
}

function sectionAcceptanceRealData() {
  return `${vModelHeader('验收测试', '真实数据验证', 'validation', '')}

## 目标

使用**实际数据**（或经批准的脱敏生产数据）进行验收。

## 注意

- 数据隐私与合规审批
- 回滚与数据清理计划

${cabPlaceholder()}`;
}

function sectionAcceptanceCriteria() {
  return `${vModelHeader('验收测试', '验收结论与签字', 'validation', '')}

## 目标

确认系统符合客户需求，且**已可在实际环境下使用**；完成用户签字。

## 结论类型

- 通过
- 有条件通过（遗留项与期限）
- 不通过

${cabPlaceholder()}`;
}

function sectionAcceptanceRelease() {
  return `${vModelHeader('验收测试', '投产与移交', 'validation', '')}

## 目标

上线、回滚方案、运维移交与交付物清单。

${deliverablesTable([
  ['可执行程序/镜像', '版本号、构建号', '开发'],
  ['用户文档', '手册、FAQ', '产品'],
  ['运维文档', '监控、告警、备份', '运维'],
  ['基线文档包', 'URD至测试报告', 'PM']
])}

${cabPlaceholder()}`;
}

// Appendix
function sectionAppendixMapping() {
  return `${vModelHeader('附录', 'V 模型对应关系表', 'meta', '')}

## 完整映射表

| 项目定义阶段（左侧） | 交付物 | 确认阶段（右侧） | 测试计划制定时点 |
| --- | --- | --- | --- |
| 需求分析 | URD、功能需求 | 用户验收测试 | **需求分析阶段** |
| 系统设计 | 软件规格说明书、实体图 | 系统测试 | **系统设计阶段** |
| 架构设计 | 架构、接口契约 | 集成测试 | **架构设计阶段** |
| 模组设计 | 低阶设计、程序规格书 | 单元测试 | **模组设计阶段** |
| 代码实现 | 源代码、构建产物 | （为右侧测试提供对象） | — |

## 追溯示意

需求 ID → 规格功能 → 架构接口 → 模组 I/O → UT/IT/ST/UAT 用例

${cabPlaceholder()}`;
}

function sectionAppendixTemplates() {
  return `${vModelHeader('附录', '文档模板清单', 'meta', '')}

| 模板 | 阶段 | 文件名建议 |
| --- | --- | --- |
| 用户需求文件 | 需求分析 | URD-vX.Y.md |
| 用户验收测试计划 | 需求分析 | UAT-Plan-vX.Y.md |
| 软件规格说明书 | 系统设计 | SRS-vX.Y.md |
| 系统测试计划 | 系统设计 | STP-vX.Y.md |
| 集成测试计划 | 架构设计 | ITP-vX.Y.md |
| 低阶设计/程序规格书 | 模组设计 | LDD-vX.Y.md |
| 单元测试计划 | 模组设计 | UTP-vX.Y.md |

${cabPlaceholder()}`;
}

function sectionAppendixChangelog() {
  return `${vModelHeader('附录', '变更记录', 'meta', '')}

| 版本 | 日期 | 变更说明 | 影响阶段 | 批准人 |
| --- | --- | --- | --- | --- |
| 0.1.0 | - | 初稿 | - | - |

${cabPlaceholder()}`;
}

function sectionAppendixBibliography() {
  return `${vModelHeader('附录', '参考资料', 'meta', '')}

- [V 模型（软件开发）— 维基百科](https://zh.wikipedia.org/wiki/V%E6%A8%A1%E5%9E%8B_(%E8%BB%9F%E9%AB%94%E9%96%8B%E7%99%BC))
- Kevin Forsberg & Harold Mooz — The Relationship of System Engineering to the Project Cycle
- Roger S. Pressman — Software Engineering: A Practitioner's Approach
- Boris Beizer — Software Testing Techniques

${cabPlaceholder()}`;
}

function sectionAppendixApproval() {
  return `${vModelHeader('附录', '审批记录', 'meta', '')}

| 阶段 | 评审日期 | 参与者 | 结论 | 签字 |
| --- | --- | --- | --- | --- |
| 需求基线 | - | - | - | - |
| 系统设计 | - | - | - | - |
| 架构设计 | - | - | - | - |
| 模组设计 | - | - | - | - |
| 验收 | - | - | - | - |

${cabPlaceholder()}`;
}

// ── Generate files ──────────────────────────────────────────────

if (fs.existsSync(contentDir)) {
  fs.rmSync(contentDir, { recursive: true, force: true });
}
fs.mkdirSync(contentDir, { recursive: true });

let count = 0;
for (const chapter of chapters) {
  const chapterDir = path.join(contentDir, chapter.id);
  fs.mkdirSync(chapterDir, { recursive: true });

  chapter.sections.forEach((section, index) => {
    const frontmatter = `---
title: ${section.title}
description: ${section.description}
chapter: ${chapter.id}
order: ${index + 1}
---

`;
    const filePath = path.join(chapterDir, `${section.slug}.md`);
    fs.writeFileSync(filePath, frontmatter + section.body, 'utf8');
    count += 1;
  });
}

console.log(`Generated ${count} articles in ${contentDir}`);
