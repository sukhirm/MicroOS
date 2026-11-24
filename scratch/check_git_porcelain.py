import subprocess

out = subprocess.check_output(["git", "status", "--porcelain"], text=True)
print(out)
