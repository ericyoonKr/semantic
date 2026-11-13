import re

EMOJI = re.compile(
    "[\U0001F000-\U0001FAFF\U00002600-\U000026FF\U00002700-\U000027BF\U0001F1E6-\U0001F1FF️‍⭐⭕⬛⬜⤴⤵]"
)


def strip_emoji(text: str) -> str:
    return EMOJI.sub("", text)
