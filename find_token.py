import subprocess
import sys
import string
import platform


def strings(filename, min=4):
    with open(filename, errors="ignore") as f:
        result = ""
        for c in f.read():
            if c in string.printable:
                result += c
                continue
            if len(result) >= min:
                yield result
            result = ""
        if len(result) >= min:
            yield result


def run_command(cmd):
    output = subprocess.Popen(
        [cmd], shell=True, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL
    )
    return output.communicate()[0].decode().splitlines()


def main():
    print("Searching for Discord localstorage databases...")
    # First, we search for .ldb files, these are the leveldb files used by chromium to store localstorage data,
    # which contains the discord token.
    rg = False
    if platform.system() == "Darwin":
        # If on macOS, use the system file cache
        results = run_command("mdfind \"kMDItemDisplayName=='*.ldb'\"")
    else:
        # Try and use ripgrep, because it's much faster, otherwise, fallback to `find`.
        try:
            subprocess.check_output(["rg", "--version"])
            results = run_command("rg ~/ --files -g '*.ldb'")
            rg = True
        except FileNotFoundError:
            results = run_command("find ~/ -name '*.ldb'")

    if len(results) == 0 and rg:
        # Try again, but search hidden directories.
        results = run_command("rg ~/ --hidden --files -g '*.ldb'")

    if len(results) == 0:
        print("No databases found.")
        sys.exit(1)

    # Only search for tokens in ldb files likely belonging to a discord applications local storage
    # (this prevents searching browsers, but browser localstorage returns lots of false positives).
    discord_databases = list(
        filter(lambda x: "discord" in x and "Local Storage" in x, results)
    )

    # Collect strings that look like discord tokens.
    token_candidates = set()
    for database in discord_databases:
        for candidates in map(lambda s: s.split(), strings(database, 40)):
            for candidate in candidates:
                if len(candidate) < 15:
                    continue
                if " " in candidate:
                    continue
                parts = candidate.split(".")
                if len(parts) != 3:
                    continue
                if len(parts[1]) < 6:
                    continue
                token_candidates.add(candidate[1:-1])

    if len(token_candidates) == 0:
        print("No Discord tokens found")
        return

    print("Possible Discord tokens found:\n")
    for token in token_candidates:
        print(token)


if __name__ == "__main__":
    main()
