---
name: "build-hotel-itinerary"
description: "Research and catalog hotel options for multi-day trips. Produces a Word document with a table of 2-3 hotel candidates per day, including pricing and amenities. Uses web search for hotel research and the 'doc' skill for DOCX output."
---

# Build Hotel Itinerary Skill

Research hotel options for a trip and produce a well-formatted Word document with recommendations organized by day.

> **Note:** This skill uses web search for hotel research and depends on the `doc` skill for DOCX creation.

## Constraints

### Number of People / Rooms
- **Default:** Assume 2 people per room
- If the user specifies N people, calculate rooms as `ceil(N / 2)`
- Example: 4 people = 2 rooms, 5 people = 3 rooms
- When booking multiple rooms, look for **identical or adjacent rooms** when possible
- Always note the room count assumption in the output

### Amenities
- If the user specifies required amenities (kitchen, washer/dryer, pool, gym, parking, etc.), these requirements **carry through for the entire trip** unless the user explicitly changes them
- Track amenities as a persistent filter — do not forget mid-search
- Common amenity keywords to watch for:
  - Kitchen / kitchenette
  - Washer / dryer / laundry
  - Free parking
  - Pool / gym / fitness
  - Pet-friendly
  - Free breakfast
  - WiFi (usually assumed)

### Budget Level
- **Default:** $150-200 per person per night (so $300-400/room for 2 people)
- If user specifies "budget" or a lower amount, adjust accordingly
- Budget tiers:
  - **Budget:** $50-100/room/night
  - **Mid-range:** $100-200/room/night  
  - **Default/Higher-end:** $150-200/person/night ($300-400/room)
  - **Luxury:** $300+/person/night
- Always show price **per room per night** in the output for clarity

### Output Format
- Produce a **Word document (.docx)** using the `doc` skill
- Include a **table indexed by day**, sorted in chronological order (Day 1 first)
- Provide **2-3 hotel candidates per day** with:
  - Hotel name
  - Nightly rate (per room)
  - Booking URL (direct link to the hotel listing)
  - Key amenities
  - Brief notes (location, rating, standout features)
- Within each day, sort candidates by **price descending** (most expensive first, cheapest last)

## User-Provided Itinerary

If the user attaches an itinerary (PDF, image, document, or text), **treat it as ground truth**:

- **Do NOT deviate** from the dates, destinations, or order specified
- **Do NOT suggest** alternative cities, routes, or trip modifications
- Use the itinerary exactly as provided to search for hotels
- Only search for accommodations that match the given locations and dates
- If the itinerary is unclear on a detail (e.g., exact dates), ask for clarification rather than guessing

The user's itinerary represents their confirmed travel plans. Your job is to find hotel options that fit their plan, not to improve or modify the plan.

**Language:** Match the language of the output document to the language of the attachment. If the user sends an itinerary in Chinese, produce the Word document in Chinese. If in English, use English. This applies to all text: headers, table content, notes, and labels.

## Workflow

1. **Gather Trip Details**
   - Destination(s) — may be multiple cities
   - Dates (check-in / check-out for each stop)
   - Number of travelers
   - Required amenities (if any)
   - Budget preference (if specified)

2. **Search for Hotels**
   Use web search to find hotel options. Search queries should include:
   - City + dates
   - Amenity requirements
   - Budget range
   
   Example searches:
   ```
   "hotels in [City] [Month] 2026 with kitchen under $200"
   "[City] hotels with washer dryer [dates]"
   "best hotels near [landmark] [City]"
   ```
   
   Sources to check:
   - Google Hotels
   - Booking.com
   - Hotels.com
   - Airbnb (for kitchen/laundry requirements)
   - Expedia

3. **Compile Candidates**
   For each night/location, identify 2-3 strong options:
   - Verify amenities match requirements
   - Confirm price is within budget
   - Note cancellation policy if visible
   - Check ratings/reviews

4. **Generate Word Document**

   ```python
   from docx import Document
   from docx.shared import Pt, Inches
   from docx.enum.table import WD_TABLE_ALIGNMENT
   from docx.enum.text import WD_ALIGN_PARAGRAPH
   
   doc = Document()
   
   # Title
   title = doc.add_heading("Hotel Itinerary", level=0)
   
   # Trip summary
   doc.add_paragraph(f"Destination: [City/Cities]")
   doc.add_paragraph(f"Dates: [Start] - [End]")
   doc.add_paragraph(f"Travelers: [N] people ([M] rooms)")
   doc.add_paragraph(f"Budget: $[X]-[Y] per room/night")
   if amenities:
       doc.add_paragraph(f"Required Amenities: [list]")
   
   doc.add_paragraph()  # spacing
   
   # Table with columns: Day | Hotel | Price/Night | Booking URL | Amenities | Notes
   table = doc.add_table(rows=1, cols=6)
   table.style = 'Table Grid'
   
   # Header row
   headers = ["Day", "Hotel", "Price/Night", "Booking URL", "Amenities", "Notes"]
   hdr_cells = table.rows[0].cells
   for i, header in enumerate(headers):
       hdr_cells[i].text = header
       hdr_cells[i].paragraphs[0].runs[0].bold = True
   
   # Add rows for each day's options
   # Day 1 - Option 1 (best)
   # Day 1 - Option 2
   # Day 1 - Option 3
   # Day 2 - Option 1 (best)
   # ...
   
   doc.save("Hotel_Itinerary_[Destination].docx")
   ```

5. **Review Output**
   - Ensure table is readable and well-formatted
   - Verify all days are covered
   - Confirm amenities are consistent throughout
   - Check that prices are within stated budget

## Example Output Table

| Day | Hotel | Price/Night | Booking URL | Amenities | Notes |
|-----|-------|-------------|-------------|-----------|-------|
| Day 1 (Apr 10) - Seattle | The Edgewater | $310 | booking.com/... | Waterfront, Gym | On the water, 4.4 stars |
| Day 1 (Apr 10) - Seattle | Hyatt Regency Seattle | $289 | hyatt.com/... | Pool, Gym, WiFi | Downtown, 4.5 stars |
| Day 1 (Apr 10) - Seattle | Motif Seattle | $265 | hotels.com/... | Gym, Restaurant | Belltown, 4.3 stars |
| Day 2 (Apr 11) - Portland | The Nines | $275 | thenines.com/... | Pool, Gym, Spa | Luxury, 4.6 stars |
| Day 2 (Apr 11) - Portland | Hotel deLuxe | $245 | hoteldeLuxe.com/... | Gym, Breakfast | Downtown, 4.5 stars |
| ... | ... | ... | ... | ... |

## Multi-City Trips

For trips spanning multiple cities:
- Group recommendations by city/location
- Add a section header for each city
- Note travel days where no hotel is needed (if driving between cities)
- Consider proximity to next destination for the last night in each city

## Output Filename

Save as: `Hotel_Itinerary_[PrimaryDestination]_[StartDate].docx`

Example: `Hotel_Itinerary_Seattle_Apr2026.docx`
