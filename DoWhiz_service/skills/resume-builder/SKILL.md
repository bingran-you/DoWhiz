---
name: "resume-builder"
description: "Build professional one-page resumes in Word (DOCX) format following a standardized template. Gathers user information and generates a properly formatted resume. Uses the 'doc' skill for DOCX rendering."
---

# Resume Builder Skill

Build professional, one-page resumes in Microsoft Word (DOCX) format.

> **Note:** This skill depends on the `doc` skill for DOCX creation and rendering. Use `python-docx` for document generation and the `doc` skill's rendering workflow for visual verification.

## Constraints

- **Always one page** - Never exceed one page. Prioritize recent/relevant experience.
- **Always use Word (DOCX)** - Use the `doc` skill workflow with `python-docx`.
- **Follow the exact format below** - Do not deviate from this structure.

## Resume Format (Exact Template)

```
                   [Full Name]
          [Phone] | [Email] | [City, State]

EDUCATION
[University Name]                                    [City, State]
[Degree]; [Major/Minor]          [Expected/Graduated Date]
• GPA: X.XX / 4.00; Honors: [honors if any]
• Coursework: [Relevant courses, comma-separated]

PROFESSIONAL EXPERIENCE
[Company Name]                                       [City, State]
[Job Title]                                         [Start Date – End Date]
• [Achievement/responsibility with quantifiable impact]
• [Achievement/responsibility with quantifiable impact]
• [Achievement/responsibility with quantifiable impact]

[Company Name]                                       [City, State]
[Job Title]                                         [Start Date – End Date]
• [Achievement/responsibility]
• [Achievement/responsibility]

PROJECTS
[Project Name]
• [Description with technical details and impact]
• [Description with technical details and impact]

[Project Name]
• [Description with technical details and impact]

SKILLS, ACTIVITIES, & INTERESTS
Skills: [Technical skills, comma-separated]
Languages: [Programming languages with frameworks in parentheses]
Interests: [Personal interests, activities]
```

## Formatting Specifications

### Font & Margins
- Font: Times New Roman or similar serif font
- Name: 18-20pt, bold, centered
- Contact line: 10-11pt, centered
- Section headers: 11pt, bold, ALL CAPS
- Body text: 10-11pt
- Margins: 0.5-0.75 inches all sides

### Spacing (Critical)

**Within a single experience block (company name → title → bullets):** NO extra spacing. These elements should be tight with 0pt space between them.

**Between separate experiences:** 5pt space ONLY. Add this after the last bullet of one experience, before the next company name.

**Between sections:** 8-10pt space before each section header.

```
[Company A]                    ← 0pt after
[Title A]                      ← 0pt after  
• Bullet 1                     ← 0pt after
• Bullet 2                     ← 5pt after (end of experience block)

[Company B]                    ← starts here, 5pt gap above
[Title B]                      ← 0pt after
• Bullet 1                     ← 0pt after
```

In `python-docx`:
```python
from docx.shared import Pt

# Within an experience: no extra spacing
company_para.paragraph_format.space_after = Pt(0)
title_para.paragraph_format.space_after = Pt(0)
bullet_para.paragraph_format.space_after = Pt(0)

# ONLY the last bullet of an experience gets 5pt after
last_bullet.paragraph_format.space_after = Pt(5)

# Section headers get space before
section_header.paragraph_format.space_before = Pt(8)
```

### Line Length (Important)

**Maximize line usage.** Avoid dangling text where a few words wrap onto the next line. If a bullet point has 2-3 words orphaned on a new line:
- Rephrase to fit on one line, OR
- Expand the content to better fill the second line

Short orphaned lines waste vertical space and look unpolished.

### Section Headers
- ALL CAPS, bold
- **Horizontal line underneath (required)** - Add a thin border/rule directly below each section header

In `python-docx`, add a bottom border to the section header paragraph:
```python
from docx.oxml.ns import qn
from docx.oxml import OxmlElement

def add_bottom_border(paragraph):
    """Add a thin horizontal line under a paragraph."""
    pPr = paragraph._p.get_or_add_pPr()
    pBdr = OxmlElement('w:pBdr')
    bottom = OxmlElement('w:bottom')
    bottom.set(qn('w:val'), 'single')
    bottom.set(qn('w:sz'), '6')  # Border thickness (1/8 pt units, so 6 = 0.75pt)
    bottom.set(qn('w:space'), '1')
    bottom.set(qn('w:color'), '000000')
    pBdr.append(bottom)
    pPr.append(pBdr)

# Usage:
section_header = doc.add_paragraph("PROFESSIONAL EXPERIENCE")
section_header.runs[0].bold = True
add_bottom_border(section_header)
```

### Company/School Lines
- **Name** bold, left-aligned
- **Location** right-aligned on same line

### Title/Degree Lines
- *Title/Degree* italic, left-aligned
- *Dates* right-aligned on same line

### Bullet Points
- Use solid bullet (•)
- Start with strong action verbs
- Include quantifiable metrics when possible
- Include as many bullets as needed to showcase the role — the only constraint is the one-page limit

## Workflow

1. **Gather Information**
   - Ask user for: contact info, education, work experience, projects, skills
   - Or ask user to provide existing resume/LinkedIn to extract from
   - **If information is missing, use placeholders** (see below) — don't block on incomplete data

2. **Validate Content**
   - Check that experience uses action verbs
   - Verify dates are formatted consistently (Mon YYYY – Mon YYYY or Mon YYYY – Present)
   - Leave placeholders for any missing fields

## Placeholders

If the user doesn't provide all information, insert bracketed placeholders so they can fill in later:

```
[Your Name]
[Phone] | [Email] | [City, State]

EDUCATION
[University Name]                                    [City, State]
[Degree]; [Major]                                   [Expected Date]
• GPA: [X.XX] / 4.00
• Coursework: [Course 1, Course 2, ...]

PROFESSIONAL EXPERIENCE
[Company Name]                                       [City, State]
[Job Title]                                         [Start – End]
• [Describe your achievement with metrics]
• [Describe another responsibility]
```

**Placeholder format:** `[Description of what goes here]`

Do NOT leave sections empty — always include placeholder text so the user knows what to fill in.

3. **Generate DOCX**
   ```python
   from docx import Document
   from docx.shared import Pt, Inches
   from docx.enum.text import WD_ALIGN_PARAGRAPH
   
   doc = Document()
   
   # Set margins
   for section in doc.sections:
       section.top_margin = Inches(0.5)
       section.bottom_margin = Inches(0.5)
       section.left_margin = Inches(0.6)
       section.right_margin = Inches(0.6)
   
   # Name (centered, bold, large)
   name_para = doc.add_paragraph()
   name_para.alignment = WD_ALIGN_PARAGRAPH.CENTER
   name_run = name_para.add_run("Full Name")
   name_run.bold = True
   name_run.font.size = Pt(18)
   
   # Contact line (centered)
   contact_para = doc.add_paragraph()
   contact_para.alignment = WD_ALIGN_PARAGRAPH.CENTER
   contact_para.add_run("+1 (555) 123-4567 | email@example.com | City, State")
   
   # ... continue with sections
   
   doc.save("resume.docx")
   ```

4. **Review & Iterate**
   - Check page count (must be exactly 1)
   - If over 1 page: reduce bullet points, trim older experience, condense
   - Offer to adjust formatting or content

## Action Verbs for Bullets

**Engineering/Technical:** Architected, Built, Deployed, Designed, Developed, Engineered, Implemented, Integrated, Optimized, Refactored

**Leadership:** Directed, Led, Managed, Mentored, Oversaw, Spearheaded

**Analysis:** Analyzed, Assessed, Benchmarked, Evaluated, Researched

**Improvement:** Automated, Enhanced, Improved, Reduced, Streamlined

## One-Page Constraint

**Fill the page.** Include all relevant experience, projects, and skills. Do not artificially limit content — use the full page.

**If content exceeds one page**, trim in this order:
1. Remove oldest/least relevant experience first
2. Trim verbose bullet points (tighten wording, not remove)
3. Remove less relevant coursework
4. Condense project descriptions
5. Reduce margins slightly (min 0.5 inches)

**Do NOT** pre-emptively limit bullets or content. The goal is a full, dense one-page resume.

## Output

Save the resume as `[LastName]_[FirstName]_Resume.docx` in the workspace.
