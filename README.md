# musicbrainz-release-grabber
Finds new albums released on musicbrainz from a given directory.

# Usage
- Load your music directory with init -d <DIR>
- Remove directories you do not want to include.
- Get the ids from musicbrainz (init -f). This will output artists where the musicbrainz artist does not match your search string. These might be wrong artists or just different spellings.
- Use new to get new albums.

Notice the last step will take a while rougly 2 seconds per as per musicbrainz throttling restrictions.
This will also obviously give all possible information you are searching for to musicbrainz.
