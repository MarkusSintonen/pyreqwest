import os
import platform

IS_CI = os.environ.get("CI") is not None
IS_OSX = platform.system() == "Darwin"
IS_WINDOWS = platform.system() == "Windows"
