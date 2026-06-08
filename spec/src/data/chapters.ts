export interface SectionDef {
  slug: string;
  title: string;
  description: string;
  /** V 模型右臂对应小节（左臂 → 右臂） */
  validates?: string;
}

export interface ChapterDef {
  id: string;
  title: string;
  order: number;
  /** V 模型相位 */
  phase: 'meta' | 'definition' | 'implementation' | 'validation';
  /** 左臂章节 id → 右臂验证章节 id */
  validationOf?: string;
  sections: SectionDef[];
}

/**
 * 章节结构严格对应维基百科 V 模型：
 * https://zh.wikipedia.org/wiki/V模型_(軟體開發)
 *
 * 左臂（项目定义）：需求分析 → 系统设计 → 架构设计 → 模组设计
 * V 底：代码实现
 * 右臂（确认）：单元测试 → 集成测试 → 系统测试 → 用户验收测试
 *
 * 各章小节数量按该阶段维基条目所列交付物确定，不凑固定篇数。
 */
export const chapters: ChapterDef[] = [
  // ── 元信息（非 V 模型本体，用于阅读导航）──
  {
    id: 'preface',
    title: '前言',
    order: 0,
    phase: 'meta',
    sections: [
      {
        slug: 'v-model-overview',
        title: 'V 模型概述',
        description: '维基百科 V 模型左右两臂与 CAB 文档对应关系'
      },
      {
        slug: 'document-system',
        title: '文档体系与追溯',
        description: '各章交付物如何形成定义—实现—确认闭环'
      },
      {
        slug: 'glossary',
        title: '术语与缩写',
        description: 'verification、validation、URD、规格书等统一用语'
      }
    ]
  },

  // ── 左臂：项目定义阶段 ──

  {
    id: 'requirements',
    title: '需求分析',
    order: 1,
    phase: 'definition',
    validationOf: 'acceptance',
    sections: [
      {
        slug: 'user-needs',
        title: '用户需要分析',
        description: '与用户面谈，分析需要，建构理想系统目标（不涉及设计方式）',
        validates: 'acceptance/uat-execution'
      },
      {
        slug: 'user-requirements-document',
        title: '用户需求文件',
        description: '编制 URD，作为验收测试的源头依据',
        validates: 'acceptance/acceptance-criteria'
      },
      {
        slug: 'functional-requirements',
        title: '功能需求整理',
        description: '整理可验证的系统功能需求',
        validates: 'acceptance/acceptance-criteria'
      },
      {
        slug: 'uat-plan',
        title: '用户验收测试计划',
        description: '在需求阶段早期制定，由企业用户在验收阶段执行',
        validates: 'acceptance/uat-execution'
      }
    ]
  },
  {
    id: 'system-design',
    title: '系统设计',
    order: 2,
    phase: 'definition',
    validationOf: 'system-test',
    sections: [
      {
        slug: 'software-specification',
        title: '软件规格说明书',
        description: '开发蓝图：架构、菜单、数据结构等高层设计',
        validates: 'system-test/system-test-execution'
      },
      {
        slug: 'architecture-and-ui',
        title: '系统架构与界面结构',
        description: '大致系统架构、指令菜单结构与主要交互',
        validates: 'system-test/functional-verification'
      },
      {
        slug: 'business-scenarios',
        title: '业务场景与样例',
        description: '业务场景、样例视窗与报表，帮助理解系统行为',
        validates: 'system-test/functional-verification'
      },
      {
        slug: 'entity-data-dictionary',
        title: '实体图与数据字典',
        description: '实体图、数据字典等技术文件',
        validates: 'system-test/functional-verification'
      },
      {
        slug: 'system-test-plan',
        title: '系统测试计划',
        description: '在系统设计阶段整理系统测试文件与通过准则',
        validates: 'system-test/system-test-execution'
      }
    ]
  },
  {
    id: 'architecture',
    title: '架构设计',
    order: 3,
    phase: 'definition',
    validationOf: 'integration-test',
    sections: [
      {
        slug: 'subsystem-decomposition',
        title: '子系统划分',
        description: '将系统拆分为可集成的子系统或服务单元',
        validates: 'integration-test/integration-test-execution'
      },
      {
        slug: 'integration-strategy',
        title: '集成策略',
        description: '集成顺序、持续集成与组装方式',
        validates: 'integration-test/integration-test-execution'
      },
      {
        slug: 'interface-contracts',
        title: '接口与通信契约',
        description: '子系统间协议、消息格式、时序与错误约定',
        validates: 'integration-test/interface-testing'
      },
      {
        slug: 'integration-test-plan',
        title: '集成测试计划',
        description: '在架构设计阶段规划集成测试项与通过标准',
        validates: 'integration-test/integration-test-execution'
      }
    ]
  },
  {
    id: 'modules',
    title: '模组设计',
    order: 4,
    phase: 'definition',
    validationOf: 'unit-test',
    sections: [
      {
        slug: 'module-decomposition',
        title: '模组拆解',
        description: '低阶设计：拆解为可编码的最小交付单元',
        validates: 'unit-test/unit-test-execution'
      },
      {
        slug: 'logic-pseudocode',
        title: '逻辑细节与伪代码',
        description: '模组内部逻辑、算法与伪代码',
        validates: 'unit-test/isolation-testing'
      },
      {
        slug: 'database-tables',
        title: '数据库表设计',
        description: '表结构、字段型别、长度与约束（CAB 为逻辑表/settings）',
        validates: 'unit-test/isolation-testing'
      },
      {
        slug: 'api-dependencies-io',
        title: '接口、依赖与输入输出',
        description: 'API 细节、相依性、错误消息、模组 I/O 列表',
        validates: 'unit-test/isolation-testing'
      },
      {
        slug: 'unit-test-plan',
        title: '单元测试计划',
        description: '在模组设计阶段规划单元测试，消除代码/单元级错误',
        validates: 'unit-test/unit-test-execution'
      }
    ]
  },

  // ── V 底：实现 ──

  {
    id: 'implementation',
    title: '代码实现',
    order: 5,
    phase: 'implementation',
    sections: [
      {
        slug: 'implementation-baseline',
        title: '实现依据与顺序',
        description: '依据低阶设计/程序规格书开展编码'
      },
      {
        slug: 'coding-standards',
        title: '编码规范',
        description: '命名、格式、异常处理、日志与可测试性约定'
      },
      {
        slug: 'build-ci',
        title: '构建与持续集成',
        description: '编译、打包、静态检查与 CI 流水线'
      },
      {
        slug: 'configuration',
        title: '配置与密钥管理',
        description: '环境配置、特性开关与密钥安全存储'
      }
    ]
  },

  // ── 右臂：确认阶段 ──

  {
    id: 'unit-test',
    title: '单元测试',
    order: 6,
    phase: 'validation',
    sections: [
      {
        slug: 'unit-test-execution',
        title: '单元测试计划执行',
        description: '执行模组设计阶段制定的单元测试计划'
      },
      {
        slug: 'isolation-testing',
        title: '隔离测试',
        description: '验证最小程式体在与其他程序隔离时能否正常运作'
      },
      {
        slug: 'unit-test-report',
        title: '单元测试报告',
        description: '通过率、遗留缺陷与对集成测试的输入'
      }
    ]
  },
  {
    id: 'integration-test',
    title: '集成测试',
    order: 7,
    phase: 'validation',
    sections: [
      {
        slug: 'integration-test-execution',
        title: '集成测试计划执行',
        description: '执行架构设计阶段制定的集成测试计划'
      },
      {
        slug: 'interface-testing',
        title: '接口联调验证',
        description: '验证子系统/模组间接口契约与时序'
      },
      {
        slug: 'integration-test-report',
        title: '集成测试报告',
        description: '集成结论、风险项与系统测试准入评估'
      }
    ]
  },
  {
    id: 'system-test',
    title: '系统测试',
    order: 8,
    phase: 'validation',
    sections: [
      {
        slug: 'system-test-execution',
        title: '系统测试计划执行',
        description: '执行系统设计阶段制定的系统测试计划'
      },
      {
        slug: 'functional-verification',
        title: '功能验证',
        description: '端到端验证软件规格说明书中的功能与界面行为'
      },
      {
        slug: 'nonfunctional-verification',
        title: '非功能验证',
        description: '性能、安全、可用性、兼容性等质量属性'
      },
      {
        slug: 'system-test-report',
        title: '系统测试报告',
        description: '系统测试结论与验收测试准入建议'
      }
    ]
  },
  {
    id: 'acceptance',
    title: '用户验收测试',
    order: 9,
    phase: 'validation',
    sections: [
      {
        slug: 'uat-execution',
        title: '用户验收测试执行',
        description: '由企业用户执行需求阶段制定的 UAT 计划'
      },
      {
        slug: 'user-environment',
        title: '用户环境验证',
        description: '在用户环境下模拟实际产品运行条件'
      },
      {
        slug: 'real-data-validation',
        title: '真实数据验证',
        description: '使用实际数据验收，确认可投产使用'
      },
      {
        slug: 'acceptance-criteria',
        title: '验收结论',
        description: '确认系统符合客户需求并满足验收标准'
      },
      {
        slug: 'release-handover',
        title: '投产与移交',
        description: '上线步骤、运维移交与交付物清单'
      }
    ]
  },

  // ── 附录 ──
  {
    id: 'appendix',
    title: '附录',
    order: 10,
    phase: 'meta',
    sections: [
      {
        slug: 'v-model-mapping',
        title: 'V 模型对应关系表',
        description: '维基百科各阶段交付物与 CAB spec 小节映射'
      },
      {
        slug: 'changelog',
        title: '变更记录',
        description: '基线变更历史与影响分析'
      },
      {
        slug: 'approval',
        title: '审批记录',
        description: '各阶段评审与签发留痕（含需求基线）'
      }
    ]
  }
];

export function findChapter(chapterId: string): ChapterDef | undefined {
  return chapters.find((c) => c.id === chapterId);
}

export function findSection(chapterId: string, sectionSlug: string): SectionDef | undefined {
  return findChapter(chapterId)?.sections.find((s) => s.slug === sectionSlug);
}
