import json
import logging
import os
import re

import httpx

logger = logging.getLogger(__name__)

DEEPSEEK_API_URL = "https://api.deepseek.com/v1/chat/completions"
MAX_PROMPT_LENGTH = 1000
MAX_TOKENS = 2000
TEMPERATURE = 0.7
MAX_GENERATIONS_PER_DAY = 10

SYSTEM_PROMPT = """You are a task planning assistant. Your ONLY job is to \
generate a structured list of tasks based on the user's description.

You MUST respond with a JSON array ONLY. No markdown, no explanation, no \
extra text.

Each task object must have exactly these fields:
- "title": a short, actionable task title (max 100 chars)
- "description": a brief description or details (max 500 chars, or null)
- "priority": an integer from 1 (highest) to 4 (lowest)

Example response:
[{"title": "Book venue", "description": "Research and book a suitable \
venue for 20 people", "priority": 1}]

Rules:
- Return between 1 and 20 tasks
- Tasks should be actionable and specific
- Order tasks logically (by priority or sequence)
- Return ONLY the JSON array, nothing else"""


class AIError(Exception):
    """Base exception for AI service errors."""

    pass


class AIRateLimitError(AIError):
    """Raised when user exceeds daily generation limit."""

    pass


class AIInputError(AIError):
    """Raised when input validation fails."""

    pass


class AIResponseError(AIError):
    """Raised when the AI response cannot be parsed."""

    pass


def is_enabled() -> bool:
    """Check if the AI feature is enabled (API key configured)."""
    return bool(os.environ.get("TODOER_DEEPSEEK_API_KEY", "").strip())


_INJECTION_PATTERNS = re.compile(
    r"(ignore\s+(previous|above|all)\s+instructions"
    r"|you\s+are\s+now"
    r"|system\s*:\s*"
    r"|<\s*/?script"
    r"|<\s*/?iframe"
    r"|javascript\s*:)",
    re.IGNORECASE,
)


def sanitize_input(prompt: str) -> str:
    """Validate and sanitize user input."""
    if not prompt or not prompt.strip():
        raise AIInputError("Prompt cannot be empty.")
    prompt = prompt.strip()
    if len(prompt) > MAX_PROMPT_LENGTH:
        raise AIInputError(
            f"Prompt too long ({len(prompt)} chars). "
            f"Maximum is {MAX_PROMPT_LENGTH}."
        )
    if _INJECTION_PATTERNS.search(prompt):
        raise AIInputError("Prompt contains disallowed content.")
    return prompt


def sanitize_output(tasks: list[dict]) -> list[dict]:
    """Sanitize and validate parsed task list."""
    sanitized = []
    for task in tasks:
        if not isinstance(task, dict):
            continue
        title = str(task.get("title", "")).strip()
        if not title:
            continue
        title = title[:100]
        description = task.get("description")
        if description is not None:
            description = str(description).strip()
            description = description[:500] if description else None
        priority = task.get("priority", 4)
        if not isinstance(priority, int) or priority < 1:
            priority = 1
        elif priority > 4:
            priority = 4
        sanitized.append(
            {
                "title": title,
                "description": description,
                "priority": priority,
            }
        )
    return sanitized[:20]


def _parse_response(text: str) -> list[dict]:
    """Parse the AI response text into a list of task dicts."""
    text = text.strip()
    # Try to extract JSON array from the response
    # Sometimes the model wraps it in markdown code blocks
    if text.startswith("```"):
        lines = text.split("\n")
        # Remove first and last lines (``` markers)
        lines = [l for l in lines if not l.strip().startswith("```")]
        text = "\n".join(lines).strip()

    try:
        data = json.loads(text)
    except json.JSONDecodeError as e:
        raise AIResponseError(f"Failed to parse AI response as JSON: {e}")

    if not isinstance(data, list):
        raise AIResponseError("AI response is not a JSON array.")
    if len(data) == 0:
        raise AIResponseError("AI returned an empty task list.")
    return data


async def generate_tasks(prompt: str, section_context: str = "") -> list[dict]:
    """Call DeepSeek V3 to generate tasks from a prompt.

    Returns a sanitized list of task dicts with keys:
    title, description, priority.
    """
    api_key = os.environ.get("TODOER_DEEPSEEK_API_KEY", "")
    if not api_key.strip():
        raise AIError("AI feature is not configured.")

    prompt = sanitize_input(prompt)

    user_message = prompt
    if section_context:
        user_message = (
            f"Context: Tasks for section '{section_context}'. " f"\n\n{prompt}"
        )

    payload = {
        "model": "deepseek-chat",
        "messages": [
            {"role": "system", "content": SYSTEM_PROMPT},
            {"role": "user", "content": user_message},
        ],
        "max_tokens": MAX_TOKENS,
        "temperature": TEMPERATURE,
    }

    try:
        async with httpx.AsyncClient(timeout=30.0) as client:
            response = await client.post(
                DEEPSEEK_API_URL,
                json=payload,
                headers={
                    "Authorization": f"Bearer {api_key}",
                    "Content-Type": "application/json",
                },
            )
    except httpx.TimeoutException:
        raise AIError("AI service timed out. Please try again.")
    except httpx.RequestError as e:
        logger.error("AI request failed: %s", type(e).__name__)
        raise AIError(
            "Failed to connect to AI service. " "Please try again later."
        )

    if response.status_code != 200:
        logger.error("AI API returned status %d", response.status_code)
        raise AIError(
            "AI service returned an error. " "Please try again later."
        )

    try:
        body = response.json()
        text = body["choices"][0]["message"]["content"]
    except (KeyError, IndexError, json.JSONDecodeError):
        raise AIResponseError("Unexpected response format from AI service.")

    raw_tasks = _parse_response(text)
    tasks = sanitize_output(raw_tasks)

    if not tasks:
        raise AIResponseError(
            "AI did not generate any valid tasks. "
            "Please try a different prompt."
        )

    logger.info(
        "AI generated %d tasks (prompt length: %d)",
        len(tasks),
        len(prompt),
    )

    return tasks
