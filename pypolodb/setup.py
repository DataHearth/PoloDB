
import tempfile
import os
import urllib.request
from os import path
from distutils.core import setup, Extension

VERSION = '0.1.0'

def get_download_url():
       return 'https://www.polodb.org/resources/0.1/lib/darwin/libpolodb_clib.a'


def download_lib():
       temp_root = tempfile.gettempdir()
       lib_root = path.join(temp_root, "polodb_lib")
       if not path.exists(lib_root):
              os.mkdir(lib_root)
       file_path = path.join(lib_root, 'libpolodb_clib.a')
       if path.exists(file_path):
              return None
       g = urllib.request.urlopen(get_download_url())
       with open(file_path, 'b+w') as f:
              f.write(g.read())
              print(file_path)

download_lib()

module1 = Extension('polodb',
                    sources = ['polodb_ext.c'],
                    extra_objects=['../target/debug/libpolodb_clib.a'])

setup (name = 'polodb',
       version = VERSION,
       description = 'This is a demo package',
       author = 'Vincent Chan',
       author_email = 'okcdz@diverse.space',
       license = 'MIT',
       ext_modules = [module1])
