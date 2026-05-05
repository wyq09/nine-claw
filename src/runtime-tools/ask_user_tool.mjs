export function createAskUserParameters(Type) {
  return Type.Object({
    title: Type.String({ minLength: 1 }),
    description: Type.Optional(Type.String()),
    questions: Type.Array(
      Type.Object(
        {
          id: Type.String({ minLength: 1 }),
          type: Type.Union([
            Type.Literal("text"),
            Type.Literal("textarea"),
            Type.Literal("single_select"),
            Type.Literal("multi_select"),
          ]),
          label: Type.String({ minLength: 1 }),
          description: Type.Optional(Type.String()),
          required: Type.Optional(Type.Boolean()),
          options: Type.Optional(
            Type.Array(
              Type.Object({
                id: Type.String({ minLength: 1 }),
                label: Type.String({ minLength: 1 }),
                description: Type.Optional(Type.String()),
              }),
            ),
          ),
          recommendedOptionId: Type.Optional(Type.String()),
          allowCustomInput: Type.Optional(Type.Boolean()),
          customInputPlaceholder: Type.Optional(Type.String()),
          placeholder: Type.Optional(Type.String()),
          maxLength: Type.Optional(Type.Number()),
          minSelections: Type.Optional(Type.Number()),
          maxSelections: Type.Optional(Type.Number()),
        },
        { additionalProperties: false },
      ),
      { minItems: 1, maxItems: 1 },
    ),
    submitLabel: Type.Optional(Type.String()),
    cancelLabel: Type.Optional(Type.String()),
    allowSkip: Type.Optional(Type.Boolean()),
    timeoutMs: Type.Optional(Type.Number({ minimum: 1000, maximum: 3600000 })),
  });
}

export function validateAskUserPolicy(input) {
  const questions = Array.isArray(input?.questions) ? input.questions : [];
  if (questions.length !== 1) {
    return ["ask_user must ask exactly one question."];
  }

  const question = questions[0];
  if (!question || typeof question !== "object") {
    return ["ask_user question is missing."];
  }

  if (question.type === "single_select" || question.type === "multi_select") {
    const errors = [];
    const options = Array.isArray(question.options) ? question.options : [];
    if (options.length < 2 || options.length > 6) {
      errors.push("ask_user choice questions must provide 2-6 options.");
    }
    const firstOption = options[0];
    const lastOption = options[options.length - 1];
    if (!firstOption || question.recommendedOptionId !== firstOption.id) {
      errors.push("ask_user must put the recommended option first.");
    }
    if (!lastOption || String(lastOption.label || "").trim() !== "其他") {
      errors.push("ask_user must end with an '其他' option.");
    }
    return errors;
  }

  return [];
}

function normalizeAnswerList(answers) {
  return Array.isArray(answers) ? answers.filter((item) => item && typeof item === "object") : [];
}

function renderAnswerValue(value) {
  if (Array.isArray(value)) {
    return value.filter((item) => typeof item === "string" && item.trim()).join(", ");
  }
  return typeof value === "string" ? value.trim() : "";
}

function buildQuestionMap(questions) {
  const map = new Map();
  for (const question of Array.isArray(questions) ? questions : []) {
    if (question && typeof question.id === "string") {
      map.set(question.id, question);
    }
  }
  return map;
}

export function formatAskUserResult(input, result) {
  const questionMap = buildQuestionMap(input?.questions);
  const normalizedAnswers = normalizeAnswerList(result?.answers);
  const lines = [
    "The user answered your clarification request.",
    "Continue the conversation now and complete the user's request using these answers. Do not stop at the tool result.",
  ];

  if (normalizedAnswers.length > 0) {
    lines.push("Answers:");
    for (const answer of normalizedAnswers) {
      const question = questionMap.get(answer.questionId);
      const label =
        typeof question?.label === "string" && question.label.trim()
          ? question.label.trim()
          : answer.questionId || "unknown_question";
      const renderedValue = renderAnswerValue(answer.value);
      const renderedCustom =
        typeof answer.customValue === "string" ? answer.customValue.trim() : "";
      const suffix = renderedCustom ? ` (补充: ${renderedCustom})` : "";
      lines.push(`- ${label}: ${renderedValue || "未填写"}${suffix}`);
    }
  }

  lines.push("Structured answers JSON:");
  lines.push(
    JSON.stringify(
      {
        widgetId: result?.widgetId,
        answers: normalizedAnswers,
      },
      null,
      2,
    ),
  );

  return lines.join("\n");
}

export function createAskUserTool(deps) {
  return {
    name: "ask_user",
    label: "Ask User",
    description:
      "Ask the user for clarification instead of guessing when key information is missing.",
    promptSnippet:
      "Use ask_user when a missing decision would materially change the result.",
    promptGuidelines: [
      "Before calling ask_user, say one short natural sentence explaining what needs clarification.",
      "Ask exactly one question per call.",
      "For choice questions, provide 2-6 options, put the recommended option first, and end with '其他'.",
      "After ask_user returns successfully, continue the task and answer the user instead of stopping.",
      "Use ask_user for ambiguity, destructive operations, or missing delegation scope.",
      "Do not fall back to numbered options in plain text.",
    ],
    parameters: createAskUserParameters(deps.Type),
    async execute(_toolCallId, input) {
      const baseUrl =
        deps.processApi?.env?.NINECLAW_PROXY_BASE_URL?.trim() ||
        process.env.NINECLAW_PROXY_BASE_URL?.trim();
      const token =
        deps.processApi?.env?.NINECLAW_PROXY_SESSION_TOKEN?.trim() ||
        process.env.NINECLAW_PROXY_SESSION_TOKEN?.trim();
      if (!baseUrl || !token) {
        return {
          content: [{ type: "text", text: "ask_user is not available — proxy not configured." }],
          details: { ok: false, reason: "no_proxy" },
        };
      }

      const errors = validateAskUserPolicy(input);
      if (errors.length > 0) {
        return {
          content: [{ type: "text", text: `ask_user policy error: ${errors.join(" ")}` }],
          details: { ok: false, reason: "invalid_policy", errors },
        };
      }

      const response = await deps.fetchImpl(`${baseUrl}/ask-user/${token}/dispatch`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(input),
      });

      if (!response.ok) {
        const text = await response.text().catch(() => "");
        return {
          content: [{ type: "text", text: `ask_user failed (HTTP ${response.status}): ${text}` }],
          details: { ok: false, reason: "http_error", status: response.status },
        };
      }

      const result = await response.json();
      if (!result?.ok) {
        return {
          content: [{
            type: "text",
            text: `ask_user did not receive a usable answer (${result?.reason || "unknown"}).`,
          }],
          details: {
            ok: false,
            reason: result?.reason || "unknown",
            widgetId: result?.widgetId,
          },
        };
      }

      return {
        content: [{ type: "text", text: formatAskUserResult(input, result) }],
        details: {
          ok: true,
          widgetId: result.widgetId,
          answers: result.answers,
        },
      };
    },
  };
}
