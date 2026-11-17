"""Make test recordings with known speakers, words and times, using the Kokoro voice model.

Two uses:

  python tools/make_fixtures.py fixtures
      Writes the two short clips that live in fixtures/ (two-speaker.wav, four-speaker.wav) and their
      truth files (*.truth.json): who spoke, when, and what they said.

  python tools/make_fixtures.py long --speakers 2 --minutes 60 --out D:/bench/interview-60.wav
  python tools/make_fixtures.py long --speakers 8 --minutes 60 --out D:/bench/focus-60.wav
      Writes a long benchmark file built from a pool of interview or focus-group turns. These are large and
      are never committed.

Needs a Python with kokoro, numpy, scipy and soundfile (for example the venv in "D:/Claude Audio Test").
Run it with -I so nothing in this folder shadows a library: python -I tools/make_fixtures.py ...

Honest limits: every voice is a computer voice reading a script. Turns are clean, the room is a soft noise
floor, and the long files reuse a pool of sentences. Real interviews (accents, poor microphones, people
talking over each other at length) will do worse; see TESTING-TODO.md.
"""

import argparse
import json
import random
import sys
from pathlib import Path

import numpy as np
import soundfile as sf
from scipy.signal import resample_poly

SR = 16000
KOKORO_SR = 24000

# Voices that are already in the local Kokoro cache. British voices need lang_code "b".
VOICES = {
    "af_heart": "a", "am_eric": "a", "bf_emma": "b", "am_onyx": "a",
    "af_kore": "a", "bm_daniel": "b", "am_echo": "a", "ef_dora": "a",
}

# ---------------------------------------------------------------------------------------------------------
# The two short fixtures. Names, places, an email and a phone number are fictional and are here on purpose:
# the de-identification passes (T6) will be tested against them.
# Each turn: (speaker, text, gap before it in seconds). A negative gap starts it before the last turn ends.
# ---------------------------------------------------------------------------------------------------------

TWO_SPEAKERS = [
    {"id": "S1", "voice": "af_heart", "role": "interviewer"},
    {"id": "S2", "voice": "am_eric", "role": "participant"},
]
TWO_TURNS = [
    ("S1", "Thanks for making time today. To start, could you tell me how long you have been teaching at Riverside College?", 0.5),
    ("S2", "Sure. This is my eleventh year. I started in the nursing program and moved over to health sciences about four years ago.", 0.7),
    ("S1", "And when the program moved online, what changed first for you?", 0.6),
    ("S2", "Honestly, the first thing was office hours. Students stopped coming. My colleague Maria Delgado set up a drop-in video room, and that helped a lot.", 0.8),
    ("S1", "What made the drop-in room work better than the old office hours?", 0.5),
    ("S2", "I think it felt less formal. You could just appear, ask one question, and leave. Nobody had to book a slot.", 0.6),
    ("S1", "Was there anything you lost in the move?", 0.7),
    ("S2", "The hallway conversations. You know, the two minutes after class when someone tells you what is really going on. That was hard to replace.", 0.9),
    ("S1", "If a student wanted to follow up with you now, how would they reach you?", 0.5),
    ("S2", "Email mostly, at d.okafor at riverside dot edu, or the department line, five five five, two one eight, four four seven zero.", 0.6),
    ("S1", "That is really helpful. Thank you.", 0.5),
]

FOUR_SPEAKERS = [
    {"id": "S1", "voice": "af_heart", "role": "moderator"},
    {"id": "S2", "voice": "am_onyx", "role": "participant"},
    {"id": "S3", "voice": "bf_emma", "role": "participant"},
    {"id": "S4", "voice": "am_echo", "role": "participant"},
]
FOUR_TURNS = [
    ("S1", "Welcome, everyone. Let's go around once. When you think about the new advising system, what comes to mind first?", 0.5),
    ("S2", "Waiting. I waited almost three weeks for my first appointment last spring.", 0.8),
    ("S3", "For me it was the opposite, actually. I got in within two days, but the advisor didn't have my records.", 0.6),
    ("S4", "Same here. I had to explain my whole transfer history again.", 0.5),
    ("S1", "So timing for some of you, and records for others. Can you say more about the records?", 0.7),
    ("S3", "Well, I transferred from Lakeview Community College, and none of those credits showed up in the portal.", 0.6),
    ("S2", "Right.", -0.3),
    ("S3", "So the advisor kept telling me I needed courses I had already taken.", 0.4),
    ("S4", "I think the portal only reads the last school you attended. That happened to my roommate, Kevin, too.", 0.6),
    ("S1", "What would have made that first meeting better?", 0.8),
    ("S2", "Honestly, a checklist before the meeting. Tell me what to bring.", 0.5),
    ("S4", "Yes, exactly.", -0.25),
    ("S3", "And let us upload transcripts ourselves, instead of waiting for the registrar.", 0.6),
    ("S1", "That's helpful. Thank you all.", 0.6),
]

# ---------------------------------------------------------------------------------------------------------
# Pools for the long benchmark files.
# ---------------------------------------------------------------------------------------------------------

INTERVIEW_QUESTIONS = [
    "Can you walk me through a typical week for you this semester?",
    "What was the hardest part of that change?",
    "How did your students react when the format changed?",
    "Who did you turn to when something went wrong?",
    "Can you give me an example of a time that worked well?",
    "What would you tell a new colleague who is starting this fall?",
    "How do you decide what to keep and what to drop from a course?",
    "Did anything surprise you about the way students used the recordings?",
    "How has your sense of your own role changed over the last few years?",
    "What support from the institution made the biggest difference?",
    "When you say it felt different, what do you mean by that?",
    "Is there anything you wish you had done sooner?",
    "How do you know when a class is going well?",
    "What does a good day look like for you now?",
    "Tell me more about that.",
    "How did that affect the rest of the department?",
]
INTERVIEW_ANSWERS = [
    "Most weeks start with planning on Sunday night, which I never used to do. I look at the discussion board, see who has gone quiet, and send a short note to those students before Monday.",
    "The hardest part was not seeing faces. In a room you can tell when people are lost. On a screen you mostly see initials in little boxes, and you have to ask much more directly.",
    "At first they were relieved, I think. Then by the middle of the term a lot of them told me they missed having a fixed time to show up. Structure turned out to matter more than convenience.",
    "Mostly the other instructors in my area. We had a group chat that was half complaints and half very practical tips, and honestly the tips saved me more than once.",
    "There was a lab session where I split them into small rooms with one clear task each, and I visited every room for five minutes. The work that came back was better than anything from the lecture weeks.",
    "I would say start small. Pick one thing to do well, like feedback on the first assignment, and build from there. Trying to redesign everything at once is how people burn out.",
    "I keep anything that students come back to, the worked examples, the short videos, the checklists. The long readings that nobody opens, I have mostly let go of.",
    "Yes, they watched them at double speed, and they skipped straight to the parts with examples. A few watched the same five minutes over and over before the exam, which told me exactly where the confusion was.",
    "I used to think of myself as the person with the answers. Now it feels more like I design the path and then stand at the hard corners, waiting to help people around them.",
    "Release time, without question. The semester they gave us a course off to redesign, the quality went up across the whole program.",
    "It is hard to put into words. Maybe that the room had a rhythm before, and online I had to create the rhythm on purpose, every week, or it just was not there.",
    "Asking students what was working, earlier. I waited until the end of term surveys, and by then it was too late to fix anything for that group.",
    "When the questions get better. At the start it is all about deadlines and grades, and when it is going well, by week five, they are asking why something works the way it does.",
    "A good day is when I close the laptop and I can name three students who learned something specific. That is enough for me.",
    "Well, it started with one small thing and kept growing. We changed one rubric, then the next course borrowed it, and within a year the whole sequence was using the same language.",
    "Some people were frustrated, and that was fair. But the newer faculty took to it quickly, and they ended up teaching the rest of us a lot.",
    "I remember one student who had been completely silent for weeks, and then she sent a long message at midnight explaining everything going on at home. After that, we found a way through together.",
    "We had a rule that every recording had to be under ten minutes. It forced us to be clear about the one thing each video was for.",
    "Not really, no. I think it took longer than anyone expected, but it got there in the end.",
    "Yes, and that is still something I think about a lot.",
]
FOCUS_PROMPTS = [
    "Let's move to the next question. How do you usually find out about events on campus?",
    "What would make that easier for you?",
    "Does anyone see it differently?",
    "Can someone give an example?",
    "How about the library hours? What has your experience been?",
    "Let's hear from someone who hasn't spoken yet.",
    "If you could change one thing about the orientation week, what would it be?",
    "How do you feel about the new parking system?",
    "Thank you. Let's keep going.",
    "What about the food options in the student center?",
    "Is there anything we haven't asked about that you want to raise?",
    "How did that make you feel?",
]
FOCUS_COMMENTS = [
    "Mostly through friends, to be honest. The emails come, but there are so many that I just skim the subject lines.",
    "I check the student app, but the calendar there is usually out of date.",
    "For me it is the flyers by the elevators. That is the only place I actually stop and read anything.",
    "I think a weekly digest would help. One message, every Monday, with everything in it.",
    "I disagree a little. I like getting separate messages, because then I can search for them later.",
    "The library closing at ten is really hard for anyone who works evenings. I usually get there at nine.",
    "I would keep orientation but make it shorter. Three full days was a lot, and half of it I forgot by the second week.",
    "The parking permits sold out in two days. I park off campus now and walk twenty minutes.",
    "The food is fine, but it is expensive, and there is nothing open after seven.",
    "I agree with what she said about the digest. That would be so much simpler.",
    "One thing nobody mentions is that the shuttle schedule changed without any notice.",
    "When I was a first year, I didn't even know the tutoring center existed until April.",
    "I feel like they assume everyone lives on campus, and most of us don't.",
    "Honestly, the best thing was the peer mentor program. My mentor answered questions at all hours.",
    "The quiet floor in the library is great, when people actually respect it.",
    "I think more seating with outlets would solve half the complaints about the student center.",
    "It made me feel like an afterthought, honestly.",
    "Same experience here, almost exactly.",
    "Can I add something to that? The same thing happened with the financial aid office.",
    "I don't have a strong opinion on that one.",
    "It has gotten better since last year, though. I want to say that.",
    "I would love some kind of text alert, just for the really important deadlines.",
]
BACKCHANNELS = ["Mm-hm.", "Right.", "Yeah.", "Exactly.", "True.", "Okay."]


# ---------------------------------------------------------------------------------------------------------

def load_pipelines():
    from kokoro import KPipeline  # imported here so --help works without it
    pipes = {}
    for code in sorted(set(VOICES.values())):
        pipes[code] = KPipeline(lang_code=code, repo_id="hexgrad/Kokoro-82M")
    return pipes


def speak(pipes, voice, text, speed=1.0):
    """Return (samples at 16 kHz, list of words with times in seconds relative to the returned audio)."""
    pipe = pipes[VOICES[voice]]
    audio_parts, words, offset = [], [], 0.0
    for result in pipe(text, voice=voice, speed=speed):
        a = result.audio.detach().cpu().numpy() if hasattr(result.audio, "detach") else np.asarray(result.audio)
        for tok in (result.tokens or []):
            if tok.start_ts is None or tok.end_ts is None or not tok.text.strip():
                continue
            if all(not c.isalnum() for c in tok.text):
                if words:  # attach punctuation to the word before it
                    words[-1]["text"] += tok.text
                continue
            words.append({"text": tok.text, "start": offset + tok.start_ts, "end": offset + tok.end_ts})
        audio_parts.append(a)
        offset += len(a) / KOKORO_SR
    audio = np.concatenate(audio_parts).astype(np.float32)
    audio = resample_poly(audio, 2, 3).astype(np.float32)  # 24 kHz -> 16 kHz
    # Trim the voice's own leading and trailing silence so a turn's edges are where the voice is.
    env = np.abs(audio)
    thr = max(env.max() * 0.02, 1e-4)
    loud = np.nonzero(env > thr)[0]
    if len(loud):
        lead = max(0, loud[0] - int(0.02 * SR))
        tail = min(len(audio), loud[-1] + int(0.05 * SR))
        audio = audio[lead:tail]
        shift = lead / SR
        for w in words:
            w["start"] = max(0.0, w["start"] - shift)
            w["end"] = max(w["start"], w["end"] - shift)
    return audio, words


def build(pipes, speakers, turns, seed, title):
    rng = random.Random(seed)
    voice_of = {s["id"]: s["voice"] for s in speakers}
    # Each speaker sits at a slightly different distance from the microphone.
    gain_of = {s["id"]: 10 ** (rng.uniform(-6, 0) / 20) for s in speakers}
    pieces, truth_turns = [], []
    cursor = 0.0
    last_end = 0.0
    for i, (spk, text, gap) in enumerate(turns):
        speed = rng.uniform(0.92, 1.08)
        audio, words = speak(pipes, voice_of[spk], text, speed)
        start = max(0.0, (last_end + gap) if gap < 0 else (cursor + gap))
        end = start + len(audio) / SR
        pieces.append((start, audio * gain_of[spk]))
        overlap = gap < 0
        truth_turns.append({
            "speaker": spk,
            "start": round(start, 3),
            "end": round(end, 3),
            "overlap": overlap,
            "text": text,
            "words": [{"text": w["text"], "start": round(start + w["start"], 3), "end": round(start + w["end"], 3)} for w in words],
        })
        if not overlap:
            cursor = end
            last_end = end
        else:
            cursor = max(cursor, end)
        if (i + 1) % 50 == 0:
            print(f"  {i + 1} turns, {cursor / 60:.1f} minutes", file=sys.stderr)
    total = cursor + 0.8
    mix = np.zeros(int(total * SR) + 1, dtype=np.float32)
    for start, audio in pieces:
        s = int(start * SR)
        mix[s:s + len(audio)] += audio
    # A soft room-noise floor (about -50 dBFS), so silence is not digital zero.
    noise = np.random.default_rng(seed).normal(0, 1, len(mix)).astype(np.float32)
    noise = np.convolve(noise, np.ones(8) / 8, mode="same")  # take the hiss off the top
    mix += noise / (np.sqrt(np.mean(noise ** 2)) + 1e-9) * 10 ** (-50 / 20)
    peak = np.max(np.abs(mix))
    if peak > 0.89:
        mix *= 0.89 / peak
    truth = {
        "title": title,
        "made_by": "tools/make_fixtures.py (Kokoro-82M voices, Apache-2.0); a computer voice reading a script",
        "seed": seed,
        "sample_rate": SR,
        "duration": round(len(mix) / SR, 3),
        "speakers": speakers,
        "turns": truth_turns,
    }
    return mix, truth


def write(out, mix, truth):
    out = Path(out)
    out.parent.mkdir(parents=True, exist_ok=True)
    sf.write(str(out), mix, SR, subtype="PCM_16")
    out.with_suffix(".truth.json").write_text(json.dumps(truth, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"wrote {out} ({truth['duration'] / 60:.1f} min, {len(truth['turns'])} turns)")


def long_script(n_speakers, minutes, seed):
    rng = random.Random(seed)
    voices = list(VOICES)[:n_speakers]
    if n_speakers == 2:
        speakers = [{"id": "S1", "voice": voices[0], "role": "interviewer"}, {"id": "S2", "voice": voices[1], "role": "participant"}]
    else:
        speakers = [{"id": "S1", "voice": voices[0], "role": "moderator"}] + [
            {"id": f"S{i + 1}", "voice": v, "role": "participant"} for i, v in enumerate(voices[1:], start=1)]
    # Words per second of Kokoro speech including the pauses, measured on the first runs (2026-11-17), used only
    # to decide when to stop adding turns.
    target_words = minutes * 60 * (3.35 if n_speakers == 2 else 3.0)
    turns, words, last = [], 0, None
    while words < target_words:
        if n_speakers == 2:
            q, a = rng.choice(INTERVIEW_QUESTIONS), rng.choice(INTERVIEW_ANSWERS)
            turns.append(("S1", q, rng.uniform(0.4, 1.2)))
            turns.append(("S2", a, rng.uniform(0.5, 1.5)))
            if rng.random() < 0.2:
                turns.append(("S1", rng.choice(BACKCHANNELS), -rng.uniform(0.2, 0.5)))
            words += len(q.split()) + len(a.split())
        else:
            p = rng.choice(FOCUS_PROMPTS)
            turns.append(("S1", p, rng.uniform(0.4, 1.0)))
            words += len(p.split())
            for _ in range(rng.randint(2, 5)):
                spk = rng.choice([s["id"] for s in speakers[1:] if s["id"] != last])
                c = rng.choice(FOCUS_COMMENTS)
                turns.append((spk, c, rng.uniform(0.3, 1.0)))
                words += len(c.split())
                last = spk
                if rng.random() < 0.25:
                    other = rng.choice([s["id"] for s in speakers if s["id"] != spk])
                    turns.append((other, rng.choice(BACKCHANNELS), -rng.uniform(0.2, 0.6)))
    return speakers, turns


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    f = sub.add_parser("fixtures")
    f.add_argument("--out-dir", default=str(Path(__file__).resolve().parent.parent / "fixtures"))
    lg = sub.add_parser("long")
    lg.add_argument("--speakers", type=int, choices=[2, 8], required=True)
    lg.add_argument("--minutes", type=float, default=60)
    lg.add_argument("--seed", type=int, default=7)
    lg.add_argument("--out", required=True)
    args = ap.parse_args()

    pipes = load_pipelines()
    if args.cmd == "fixtures":
        d = Path(args.out_dir)
        write(d / "two-speaker.wav", *build(pipes, TWO_SPEAKERS, TWO_TURNS, 1, "Two-person interview (fixture)"))
        write(d / "four-speaker.wav", *build(pipes, FOUR_SPEAKERS, FOUR_TURNS, 2, "Four-person focus group (fixture)"))
    else:
        speakers, turns = long_script(args.speakers, args.minutes, args.seed)
        title = f"{args.minutes:g}-minute {'interview' if args.speakers == 2 else 'focus group'}, {args.speakers} voices (benchmark)"
        write(args.out, *build(pipes, speakers, turns, args.seed, title))


if __name__ == "__main__":
    main()
